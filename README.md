# Chonkit

Chunk documents.

## Contents

- [General information](#general-information)
- [Providers](#providers)
- [Building](#building)
  - [Prerequisites](#prerequisites)
    - [Pdfium](#pdfium)
    - [Fastembed](#fastembed)
    - [CUDA](#cuda)
  - [Features](#features)
  - [Sqlx 'offline' compilation](#sqlx-offline-compilation)
  - [Local quickstart](#local-quickstart)
- [Running](#running)
- [Authorization](#authorization)
- [OpenAPI documentation](#openapi-documentation)
- [License](#license)

## General information

Chonkit is an application for chunking documents
whose chunks can then be used for retrieval augmented generation (RAG).

RAG is a technique to provide LLMs contextual information about arbitrary data.
The jist of RAG is the following:

1. User sends a prompt.
2. Prompt is used for semantic search to retrieve context from the knowledge base.
3. Context and prompt are sent to LLM, providing it the necessary information to
   answer the prompt accurately.

Chonkit focuses on problem 2.

### Parsers

Documents come in many different shapes and sizes. A parser is responsible
for turning its content into bytes (raw text) and forwarding them to the chunkers.
Parsers can be configured to read only a specific range from the document,
and they can be configured to skip arbitrary text elements.

Chonkit provides an API to configure parsers for fast iteration.

### Chunkers

Embedding and retrieving whole documents is unfeasible
as they can be massive, so we need some way to split them up into
smaller parts, but still retain information clarity.

Chonkit currently offers 3 flavors of chunkers:

- SlidingWindow - the simplest (and worst performing) chunking implementation.
- SnappingWindow - a better heuristic chunker that retains sentence stops.
- SemanticWindow - an experimental chunker that uses embeddings and their
  distances to determine chunk boundaries.

The optimal flavor depends on the document being chunked.
There is no perfect chunking flavor and finding the best one will be a game of
trial and error, which is why it is important to get fast feedback when chunking.

Chonkit provides APIs to configure how documents get chunked, as well as a preview
API for fast iteration.

### Vectors

Once the documents are chunked, we have to store them somehow. We do this by
embedding them into vectors and storing them to a collection in a
vector database. Vector databases are specialised software used
for efficient storage of these vectors and their retrieval.

Chonkit provides APIs to manipulate vector collections and store embeddings
into them.

## Providers

Chonkit uses a modular architecture that allows for easy integration of new
vector database, embedding, and document storage providers.
This section lists the available providers and their corresponding feature flags.

### Vector database providers

| Provider | Feature    | Description                                              |
| -------- | ---------- | -------------------------------------------------------- |
| Qdrant   | `qdrant`   | Enable qdrant as one of the vector database providers.   |
| Weaviate | `weaviate` | Enable weaviate as one of the vector database providers. |

#### Qdrant

| Arg            | Env          | Default | Description |
| -------------- | ------------ | ------- | ----------- |
| `--qdrant-url` | `QDRANT_URL` | -       | Qdrant URL. |

#### Weaviate

| Arg              | Env            | Default | Description   |
| ---------------- | -------------- | ------- | ------------- |
| `--weaviate-url` | `WEAVIATE_URL` | -       | Weaviate URL. |

### Embedding providers

| Provider     | Feature                  | Description                                                                                                                                                                                                                                                                                                                             |
| ------------ | ------------------------ | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| OpenAI       | `openai`                 | Enable OpenAI as one of the embedding providers.                                                                                                                                                                                                                                                                                        |
| Azure OpenAI | `azure`                  | Enable Azure OpenAI as one of the embedding providers.                                                                                                                                                                                                                                                                                  |
| Fastembed    | `fe-local` / `fe-remote` | Enable Fastembed as one of the embedding providers. The local implementation uses the current machine to embed, the remote implementation uses a remote server and needs a URL to connect to. When running locally the `cuda` feature flag will enable CUDA support and will fallback to the CPU if a CUDA capable device is not found. |

#### Required arguments

#### OpenAI

\-

#### Azure

| Arg                   | Env                 | Default | Description                                                                                                       |
| --------------------- | ------------------- | ------- | ----------------------------------------------------------------------------------------------------------------- |
| `--azure-key`         | `AZURE_KEY`         | -       | Azure OpenAI API key.                                                                                             |
| `--azure-endpoint`    | `AZURE_ENDPOINT`    | -       | Azure OpenAI endpoint, including the resource, but not the deployment; e.g. `https://<resource>.openai.azure.com` |
| `--azure-api-version` | `AZURE_API_VERSION` | -       | Azure OpenAI API version.                                                                                         |

#### Remote Fastembed

| Arg            | Env          | Default | Description                                 |
| -------------- | ------------ | ------- | ------------------------------------------- |
| `--fembed-url` | `FEMBED_URL` | -       | The URL to connect to the Fastembed server. |

### Document storage providers

| Provider     | Feature         | Capabilities |
| ------------ | --------------- | ------------ |
| Local        | Always enabled. | read/write   |
| Google Drive | `gdrive`        | read         |

#### Local

Uses the machine's file system to store documents. Always enabled and cannot be disabled.

#### Google Drive

When enabled, allows files to be imported from Google Drive.

Google Drive only accepts tokens generated by OAuth clients,
therefore you need to set one up with a Google project.

To use any of the routes for importing files, an access token from
Google is required.

When accessing any of this provider's routes, the access token must either be
in the `google_drive_access_token` cookie, or in the `X-Google-Drive-Acess-Token`
header.

All files imported from Drive will be downloaded into the directory provided
on application startup (see table below). This means changes from Drive will
not be reflected in Chonkit unless manually refreshed. There is a route that
lists all files imported from Drive and compares the local modification time
with the current modification time of the file. If the external modification time
is newer, the file will be re-downloaded.

| Arg                            | Env                          | Default           | Description                                                   |
| ------------------------------ | ---------------------------- | ----------------- | ------------------------------------------------------------- |
| `--google-drive-download-path` | `GOOGLE_DRIVE_DOWNLOAD_PATH` | `./upload/gdrive` | The directory to download files to when importing from Drive. |

## Binaries

This workspace consists the following binaries:

- chonkit; exposes an HTTP API around `chonkit`'s core functionality.
- feserver; used to initiate fastembed with
  CUDA and expose an HTTP API for embeddings.

## Building

### Prerequisites

#### Pdfium

Chonkit depends on [pdfium_render](https://github.com/ajrcarey/pdfium-render)
to parse PDFs. This library depends on [libpdfium.so](https://github.com/bblanchon/pdfium-binaries).
In order for compilation to succeed, the library must be installed on the system.
To download a version of `libpdfium` compatible with chonkit (6996),
run the following (assuming Linux):

```bash
mkdir pdfium
wget https://github.com/bblanchon/pdfium-binaries/releases/download/chromium%2F6996/pdfium-linux-x64.tgz -O - | tar -xzvf - -C ./pdfium
```

The library can be found in `./pdfium/lib/libpdfium.so`.
In order to let cargo know of its existence, you have 2 options:

- Set the `LD_LIBRARY_PATH` environment variable.

  - By default, the GNU linker is set up to search for libraries in `/usr/lib`.
    If you copy the `libpdfium.so` into one of those directories, you do not
    need to need to set this variable. However, if you want to use the library
    from a different location, you need to tell the linker where it is:

    ```bash
    export LD_LIBRARY_PATH=/path/to/dir/containing/pdfium:$LD_LIBRARY_PATH
    ```

    Note: You need to pass the directory that contains the `libpdfium.so` file,
    not the file itself. This command could also be placed in your `.rc` file.

- Copy the `libpdfium.so` file to `/usr/lib`.

The latter is the preferred option as it is the least involved.

See also: [rpath](https://en.wikipedia.org/wiki/Rpath).

Note: The same procedure is applicable on Mac, only the paths and
actual library files will be different.

#### Fastembed

- Required when compiling with `fe-local`.

Fastembed models require an [onnxruntime](https://github.com/microsoft/onnxruntime).
This library can be downloaded from [here](https://github.com/microsoft/onnxruntime/releases),
or via the system's native package manager.

#### CUDA

- Required when compiling with `fe-local` and `cuda`.

If using the `cuda` feature flag with `fastembed`, the system will need to have
the [CUDA toolkit](https://developer.nvidia.com/cuda-downloads) installed.
Fastembed, and in turn `ort`, will then use the CUDA execution provider for the
onnxruntime. `ort` is designed to fail gracefully if it cannot register CUDA as
one of the execution providers and the CPU provider will be used as fallback.

Additionally, if running `feserver` with Docker, [these instructions](https://docs.nvidia.com/datacenter/cloud-native/container-toolkit/latest/install-guide.html#installation)
need to be followed to enable GPUs in Docker.

### Features

The following is a table of the supported build features.

| Feature     | Configuration      | Description                                                                                         |
| ----------- | ------------------ | --------------------------------------------------------------------------------------------------- |
| `qdrant`    | VectorDb provider  | Enable qdrant as one of the vector database providers.                                              |
| `weaviate`  | VectorDb provider  | Enable weaviate as one of the vector database providers.                                            |
| `fe-local`  | Embedder provider  | Use the implementation of `Embedder` with `LocalFastEmbedder`. Mutually exclusive with `fe-remote`. |
| `fe-remote` | Embedder provider  | Use the implementation of `Embedder` with `RemoteFastEmbedder`. Mutually exclusive with `fe-local`. |
| `openai`    | Embedder provider  | Enable openai as one of the embedding providers.                                                    |
| `azure`     | Embedder provider  | Enable azure as one of the embedding providers.                                                     |
| `cuda`      | Execution provider | Available when using `fe-local`. When enabled, uses the CUDAExecutionProvider for the onnxruntime.  |
| `gdrive`    | Storage provider   | Enable Google Drive as one of the document storage providers.                                       |
| `auth-jwt`  | Authorization      | Enable JWT authorization.                                                                           |

#### Full build command example

```bash
cargo build -F "qdrant weaviate fe-local openai azure" --release
```

### Sqlx 'offline' compilation

By default, Chonkit uses [sqlx](https://github.com/launchbadge/sqlx) with Postgres.
During compilation, sqlx will use the `DATABASE_URL` environment variable to
connect to the database. In order to prevent this default behaviour, run

```bash
cargo sqlx prepare --merged
```

This will cache the queries needed for 'offline' compilation.
The cached queries are stored in the `.sqlx` directory and are checked
into version control. You can check whether the build works by unsetting
the `DATABASE_URL` environment variable.

```bash
unset DATABASE_URL
```

### Local quickstart

```bash
cp .example.env .env
source setup.sh
```

Creates the 'data/upload' and 'data/gdrive' directories for storing documents.
Starts the infrastructure containers (postgres, qdrant, weaviate).
Exports the necessary environment variables to run chonkit.

Run

```bash
source setup.sh -h
```

to see all the available options for the setup script.

## Running

Along with provider specific arguments, Chonkit accepts the following:

| Arg                      | Env                    | Feature  | Default         | Description                                         |
| ------------------------ | ---------------------- | -------- | --------------- | --------------------------------------------------- |
| `--db-url`               | `DATABASE_URL`         | \*       | -               | The database URL.                                   |
| `--log`                  | `RUST_LOG`             | \*       | `info`          | The `RUST_LOG` env filter string to use.            |
| `--upload-path`          | `UPLOAD_PATH`          | \*       | `./upload`      | Sets the upload path of the local storage.          |
| `--address`              | `ADDRESS`              | \*       | `0.0.0.0:42069` | The address (host:port) to bind the server to.      |
| `--cors-allowed-origins` | `CORS_ALLOWED_ORIGINS` | \*       | -               | Comma separated list of origins allowed to connect. |
| `--cors-allowed-headers` | `CORS_ALLOWED_HEADERS` | \*       | -               | Comma separated list of accepted headers.           |
| `--cookie-domain`        | `COOKIE_DOMAIN`        | \*       | `localhost`     | Which domain to set on cookies.                     |
| -                        | `OPENAI_KEY`           | `openai` | -               | OpenAI API key.                                     |

The arguments have priority over the environment variables.
See `RUST_LOG` syntax [here](https://rust-lang-nursery.github.io/rust-cookbook/development_tools/debugging/config_log.html#configure-logging).
See [Authorization](#authorization) for more information about authz specific arguments.

## Authorization

### JWT authorization

#### Feature

`auth-jwt`

#### Required args

| Arg               | Env             | Default | Description            |
| ----------------- | --------------- | ------- | ---------------------- |
| `--jwt-issuer`    | `JWT_ISSUER`    | -       | The issuer of the JWT. |
| `--jwks-endpoint` | `JWKS_ENDPOINT` | -       | The JWKs endpoint.     |

#### Description

Chonkit supports standard OAuth 2.0 + OIDC authorization. It uses [jwtk](https://github.com/blckngm/jwtk) to verify tokens with their associated public keys via the
JWKs endpoint.

Along with the signature, the following claims are validated:

- `iss`: Has to be equal to the `JWT_ISSUER` starting argument.
- `entitlements`: Must contain the `admin` application entitlement.
- `groups`: Must contain the `ragu_admins` group.

## OpenAPI documentation

OpenAPI documentation is available at any chonkit instance at `http://your-address/swagger-ui`.

## License

This repository contains Chonkit, a part of Ragu, covered under the [Apache License 2.0](LICENSE), except where noted (any Ragu logos or trademarks are not covered under the Apache License, and should be explicitly noted by a LICENSE file.)

Chonkit, a part of Ragu, is a product produced from this open source software, exclusively by Barrage d.o.o. It is distributed under our commercial terms.

Others are allowed to make their own distribution of the software, but they cannot use any of the Ragu trademarks, cloud services, etc.

We explicitly grant permission for you to make a build that includes our trademarks while developing Ragu itself. You may not publish or share the build, and you may not use that build to run Ragu for any other purpose.

