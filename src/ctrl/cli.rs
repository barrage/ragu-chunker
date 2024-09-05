use std::{collections::HashMap, path::PathBuf};

use crate::{
    app::service::ServiceState,
    core::{document::parser::ParseConfig, model::Pagination},
};
use clap::{Args, Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(name = "chonkit-cli", author = "biblius", version = "0.1", about = "Chunk documents", long_about = None)]
struct CliArgs {
    #[clap(subcommand)]
    command: Execute,
}

#[derive(Debug, Subcommand)]
enum Execute {
    #[clap(subcommand)]
    Doc(DocumentExec),
    #[clap(subcommand)]
    Vec(VectorExec),
}

#[derive(Debug, Subcommand)]
enum DocumentExec {
    /// Get full config details for a single document.
    Meta(IdArg),
    /// Sync the documents repository with the document storage.
    Sync,
    /// List document metadata.
    List(ListArgs),
    /// Preview chunks for a document using its parsing and chunking config.
    Chunkp(ChunkpArg),
    /// Preview text for a document using the given parsing config.
    Parsep(ParseArg),
}

#[derive(Debug, Subcommand)]
enum ChunkMode {
    Sw(IdArg),
    Ssw(ChunkpArg),
}

#[derive(Debug, Subcommand)]
enum VectorExec {}

#[derive(Debug, Args, Default, Clone)]
struct IdArg {
    /// Document ID.
    #[arg(long, short)]
    id: uuid::Uuid,
}

#[derive(Debug, Args, Default, Clone)]
struct ParseArg {
    /// Document ID.
    #[arg(long, short)]
    id: uuid::Uuid,

    /// Skip the first `start` elements.
    #[arg(long, short)]
    start: usize,

    /// Omit the last `end` elements.
    #[arg(long, short)]
    end: usize,

    #[arg(long, short, action)]
    range: bool,

    /// Parsing specific text elements filters.
    #[arg(long, short)]
    filter: Option<String>,
}

fn csv_to_vec(csv: String) -> Vec<String> {
    csv.split(',').map(String::from).collect()
}

#[derive(Debug, Args, Default, Clone)]
struct ChunkpArg {
    /// Document ID.
    #[arg(long, short)]
    id: uuid::Uuid,

    /// The start of the range of chunks to print.
    #[arg(long, short, default_value = "0")]
    start: usize,

    /// The end of the range of chunks to print.
    #[arg(long, short, default_value = "10")]
    end: usize,

    /// If given, writes the range of chunks as json to the path.
    #[arg(long, short)]
    out: Option<PathBuf>,
}

#[derive(Debug, Args, Default, Clone)]
struct ListArgs {
    #[arg(long, short, default_value = "10")]
    limit: usize,
    #[arg(long, short, default_value = "0")]
    offset: usize,
}

pub async fn run(services: ServiceState) {
    let args = CliArgs::parse();
    match args.command {
        Execute::Doc(doc) => match doc {
            DocumentExec::Meta(IdArg { id }) => {
                let doc = services.document.get_config(id).await.unwrap();
                println!("{:#?}", doc);
            }
            DocumentExec::Sync => services.document.sync().await.unwrap(),
            DocumentExec::List(ListArgs { limit, offset }) => {
                let docs = services
                    .document
                    .list_documents(Pagination::new(limit, offset))
                    .await
                    .unwrap();
                println!("{:#?}", docs);
            }
            DocumentExec::Chunkp(ChunkpArg {
                id,
                start,
                end,
                out,
            }) => {
                let chunks = services.document.chunk_preview(id, None).await.unwrap();
                print_chunks(start, end, &chunks);
                if let Some(out) = out {
                    write_chunks(chunks, start, end, out);
                }
            }
            DocumentExec::Parsep(ParseArg {
                id,
                start,
                end,
                filter,
                range,
            }) => {
                let filters = filter.map(csv_to_vec).unwrap_or_default();
                let mut cfg = ParseConfig::new(start, end);
                if range {
                    cfg = cfg.use_range();
                }
                for filter in filters {
                    cfg = cfg.filter(regex::Regex::new(&filter).unwrap());
                }
                let parsed = services
                    .document
                    .parse_preview(id, Some(cfg))
                    .await
                    .unwrap();
                println!("{parsed}");
            }
        },
        Execute::Vec(_) => todo!(),
    }
}

fn print_chunks(start: usize, end: usize, chunks: &[String]) {
    for (i, chunk) in chunks.iter().enumerate() {
        if i < start.saturating_sub(1) {
            continue;
        }
        if i > end {
            break;
        }
        println!("Chunk {i} {:=>60}", "v");
        println!();
        println!("{chunk}");
        println!();

        println!("Total chunks: {}", chunks.len());
    }
}

fn write_chunks(chunks: Vec<String>, start: usize, end: usize, out: PathBuf) {
    let total = chunks.len();
    let chunks = chunks
        .into_iter()
        .enumerate()
        .skip(start)
        .rev()
        .skip(total - end)
        .rev()
        .collect::<HashMap<usize, String>>();

    std::fs::write(out, serde_json::to_string(&chunks).unwrap()).unwrap();
}
