//! Defines application business models.

use serde::{Deserialize, Serialize};
use serde_with::{serde_as, DisplayFromStr};
use validify::{field_err, Validate, ValidationError};

/// Vector collection models.
pub mod collection;

/// Models for embeddings present in collections.
pub mod embedding;

/// Document models.
pub mod document;

/// Image models for storage and DB.
pub mod image;

/// Used to obtain paginated lists with a total number of items in
/// the tables.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct List<T> {
    pub total: Option<usize>,
    pub items: Vec<T>,
}

impl<'s, T> utoipa::ToSchema<'s> for List<T>
where
    T: utoipa::ToSchema<'s>,
{
    fn schema() -> (
        &'s str,
        utoipa::openapi::RefOr<utoipa::openapi::schema::Schema>,
    ) {
        let (_, item_schema) = T::schema();

        let list_schema = utoipa::openapi::schema::ObjectBuilder::new()
            .title(Some("List"))
            .property(
                "total",
                utoipa::openapi::schema::ObjectBuilder::new()
                    .title(Some("total"))
                    .schema_type(utoipa::openapi::SchemaType::Integer),
            )
            .property(
                "items",
                utoipa::openapi::schema::ArrayBuilder::new().items(item_schema),
            )
            .build();

        (
            "List",
            utoipa::openapi::RefOr::T(utoipa::openapi::Schema::Object(list_schema)),
        )
    }
}

impl<T> List<T> {
    pub fn new(total: Option<usize>, items: Vec<T>) -> Self {
        Self { total, items }
    }
}

impl<T> std::iter::IntoIterator for List<T> {
    type Item = T;

    type IntoIter = std::vec::IntoIter<T>;

    fn into_iter(self) -> Self::IntoIter {
        self.items.into_iter()
    }
}

/// Used to paginate queries.
///
/// `page` defaults to 1 (which results in offset 0).
/// `per_page` defaults to 10.
#[serde_as]
#[derive(Debug, Clone, Copy, Deserialize, Validate, utoipa::ToSchema, utoipa::IntoParams)]
#[serde(rename_all = "camelCase")]
pub struct Pagination {
    /// The limit.
    #[serde_as(as = "DisplayFromStr")]
    #[validate(range(min = 1.))]
    per_page: usize,

    /// The offset.
    #[serde_as(as = "DisplayFromStr")]
    #[validate(range(min = 1.))]
    page: usize,
}

impl Pagination {
    pub fn new(per_page: usize, page: usize) -> Self {
        Self { per_page, page }
    }

    /// Returns a tuple whose first element is the LIMIT and second
    /// the OFFSET for the query.
    pub fn to_limit_offset(&self) -> (i64, i64) {
        let Self { page, per_page } = self;
        (*per_page as i64, ((page - 1) * *per_page) as i64)
    }
}

impl Default for Pagination {
    fn default() -> Self {
        Self {
            per_page: i64::MAX as usize,
            page: 1,
        }
    }
}

/// Used to paginate queries and sort the rows.
#[derive(Debug, Clone, Deserialize, Validate, utoipa::ToSchema, utoipa::IntoParams)]
#[serde(rename_all = "camelCase")]
pub struct PaginationSort<T> {
    /// See [Pagination].
    #[validate]
    #[serde(flatten)]
    pub pagination: Option<Pagination>,

    /// The column to sort by.
    ///
    // WARNING
    // Validating this field is paramount since it can be used for SQL injection.
    // Prepared statements do not support placeholders in ORDER BY clauses because they
    // use column names and not values.
    //
    /// Default: `updated_at`
    #[validate(length(min = 1, max = 64))]
    #[validate(custom(ascii_alphanumeric_column))]
    pub sort_by: Option<String>,

    /// The direction to sort in.
    ///
    /// Default: `DESC`
    pub sort_dir: Option<SortDirection>,

    #[validate]
    #[serde(flatten)]
    pub search: Option<Search<T>>,
}

impl<T> PaginationSort<T> {
    pub fn new(pagination: Pagination, sort_by: String, sort_dir: SortDirection) -> Self {
        Self {
            pagination: Some(pagination),
            sort_by: Some(sort_by),
            sort_dir: Some(sort_dir),
            search: None,
        }
    }

    pub fn new_default_sort(pagination: Pagination) -> Self {
        Self {
            pagination: Some(pagination),
            sort_by: Some("updated_at".to_string()),
            sort_dir: Some(SortDirection::Desc),
            search: None,
        }
    }

    /// Returns a tuple whose first element is the sort column and
    /// second the sort direction ASC/DESC.
    pub fn to_sort(&self) -> (&str, &str) {
        let direction = match self.sort_dir {
            Some(SortDirection::Asc) => "ASC",
            Some(SortDirection::Desc) | None => "DESC",
        };

        (self.sort_by.as_deref().unwrap_or("updated_at"), direction)
    }

    /// See [Pagination::to_limit_offset].
    pub fn to_limit_offset(&self) -> (i64, i64) {
        self.pagination
            .map(|pagination| pagination.to_limit_offset())
            .unwrap_or(Pagination::default().to_limit_offset())
    }
}

impl<T> Default for PaginationSort<T> {
    fn default() -> Self {
        Self {
            pagination: Some(Pagination::default()),
            sort_by: None,
            sort_dir: None,
            search: None,
        }
    }
}

#[derive(Debug, Clone, Copy, Deserialize, utoipa::ToSchema)]
pub enum SortDirection {
    #[serde(rename = "asc")]
    Asc,
    #[serde(rename = "desc")]
    Desc,
}

/// Struct used for search functionality when querying various models.
#[derive(Debug, Clone, Validate, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct Search<T> {
    #[validate(length(min = 1, max = 64))]
    #[validate(custom(ascii_alphanumeric_clean))]
    #[serde(alias = "search.q")]
    pub q: String,

    /// The column to search by when performing the query. This is intended to be an enum of all
    /// the possible search values for a given query.
    #[serde(alias = "search.column")]
    pub column: T,
}

/// Intended to be implemented on enums that serve as search columns.
pub trait ToSearchColumn {
    fn to_search_column(&self) -> &'static str;

    /// Useful when performing joins for prefixing the column with the desired table.
    fn to_search_column_prefixed(&self, prefix: &str) -> String {
        format!("{}.{}", prefix, self.to_search_column())
    }
}

/// Allows for strings that consist of `a-z A-Z 0-9 _-.`.
fn ascii_alphanumeric_clean(s: &str) -> Result<(), ValidationError> {
    if !s
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '.' || c == ' ')
    {
        return Err(field_err!(
            "ascii_alphanumeric_underscored",
            "parameter must be alphanumeric [a-z A-Z 0-9 _-. ]"
        ));
    }
    Ok(())
}

/// A stricter version of [ascii_alphanumeric_clean] which allows only underscored as special
/// chars.
fn ascii_alphanumeric_column(s: &str) -> Result<(), ValidationError> {
    if !s
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.')
    {
        return Err(field_err!(
            "ascii_alphanumeric_underscored",
            "parameter must be alphanumeric with underscores [a-z A-Z 0-9 _ .]"
        ));
    }
    Ok(())
}

/// Creates an enum with the specified variants that implements [ToSearchColumn].
#[macro_export]
macro_rules! search_column {
    ($name:ident, $($variant:ident => $column:literal),+ $(,)?) => {
        #[derive(Debug, serde::Deserialize, utoipa::ToSchema)]
        #[cfg_attr(test, derive(Clone))]
        #[serde(rename_all = "snake_case")]
        pub enum $name {
            $($variant),+
        }

        impl $crate::core::model::ToSearchColumn for $name {
            fn to_search_column(&self) -> &'static str {
                match self {
                    $($name::$variant => $column),+
                }
            }
        }
    };
}
