use crate::{
    core::{
        model::{
            image::{ImageModel, InsertImage},
            List,
        },
        repo::{Repository, Transaction},
        service::document::dto::ListImagesParameters,
    },
    err,
    error::ChonkitError,
    map_err,
};
use uuid::Uuid;

impl Repository {
    pub async fn insert_image(
        &self,
        insert: InsertImage<'_>,
        tx: Option<&mut Transaction<'_>>,
    ) -> Result<ImageModel, ChonkitError> {
        let query = sqlx::query_as!(
            ImageModel,
            r#"INSERT INTO images (
                document_id,
                page_number,
                image_number,
                format,
                path,
                hash,
                src,
                description,
                width,
                height
               )
               VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
               RETURNING 
                id,
                path,
                format,
                hash,
                src,
                width,
                height,
                description,
                document_id,
                page_number,
                image_number
            "#,
            insert.document_id,
            insert.page_number.map(|n| n as i32),
            insert.image_number.map(|n| n as i32),
            insert.format,
            insert.path,
            insert.hash,
            insert.src,
            insert.description,
            insert.width as i32,
            insert.height as i32
        );

        match tx {
            Some(tx) => Ok(map_err!(query.fetch_one(&mut **tx).await)),
            None => Ok(map_err!(query.fetch_one(&self.client).await)),
        }
    }

    pub async fn get_image_by_path(&self, path: &str) -> Result<ImageModel, ChonkitError> {
        Ok(map_err!(
            sqlx::query_as!(
                ImageModel,
                r#"SELECT
                id,
                path,
                page_number,
                image_number,
                format,
                hash,
                document_id,
                src,
                description,
                width,
                height
                FROM images WHERE path = $1"#,
                path
            )
            .fetch_one(&self.client)
            .await
        ))
    }

    pub async fn get_image_description(&self, path: &str) -> Result<Option<String>, ChonkitError> {
        Ok(map_err!(
            sqlx::query!("SELECT description FROM images WHERE path = $1", path)
                .fetch_optional(&self.client)
                .await
        )
        .map(|row| row.description)
        .flatten())
    }

    pub async fn update_image_description(
        &self,
        image_id: Uuid,
        description: Option<&str>,
    ) -> Result<(), ChonkitError> {
        let count = map_err!(
            sqlx::query!(
                "UPDATE images SET description = $1 WHERE id = $2",
                description,
                image_id
            )
            .execute(&self.client)
            .await
        );

        if count.rows_affected() == 0 {
            return err!(DoesNotExist, "Image does not exist");
        }

        Ok(())
    }

    /// List the images found in the document with pagination.
    ///
    /// Never use `src` from external sources; It is only used internally to include only images
    /// for the currently set image provider.
    pub async fn list_document_images(
        &self,
        src: &str,
        parameters: ListImagesParameters,
    ) -> Result<List<ImageModel>, ChonkitError> {
        let (limit, offset) = parameters.pagination.unwrap_or_default().to_limit_offset();

        let mut count = sqlx::QueryBuilder::new("SELECT COUNT(*) FROM images WHERE src = ");
        let mut data = sqlx::QueryBuilder::new(
            r#"
            SELECT
                id,
                path,
                page_number,
                image_number,
                format,
                hash,
                document_id,
                src,
                description,
                width,
                height
            FROM images WHERE src = 
            "#,
        );

        count.push_bind(src);
        data.push_bind(src);

        if let Some(document_id) = parameters.document_id {
            count.push(" AND document_id = ").push_bind(document_id);
            data.push(" AND document_id = ").push_bind(document_id);
        }

        data.push(" ORDER BY page_number, image_number")
            .push(" LIMIT ")
            .push_bind(limit)
            .push(" OFFSET ")
            .push_bind(offset);

        let total: i64 = map_err!(count.build_query_scalar().fetch_one(&self.client).await);
        let images: Vec<ImageModel> = map_err!(
            data.build_query_as::<ImageModel>()
                .fetch_all(&self.client)
                .await
        );

        Ok(List::new(Some(total as usize), images))
    }

    pub async fn list_all_document_images(
        &self,
        document_id: Uuid,
        src: &str,
    ) -> Result<Vec<ImageModel>, ChonkitError> {
        Ok(map_err!(
            sqlx::query_as!(
                ImageModel,
                r#"
                SELECT
                    id,
                    path,
                    page_number,
                    image_number,
                    format,
                    hash,
                    document_id,
                    src,
                    description,
                    width,
                    height
                FROM images WHERE document_id = $1 AND src = $2
                ORDER BY page_number, image_number"#,
                document_id,
                src
            )
            .fetch_all(&self.client)
            .await
        ))
    }

    pub async fn get_image_by_id(
        &self,
        image_id: Uuid,
    ) -> Result<Option<ImageModel>, ChonkitError> {
        Ok(map_err!(
            sqlx::query_as!(
                ImageModel,
                r#"
                SELECT 
                    id,
                    path,
                    page_number,
                    image_number,
                    format,
                    hash,
                    document_id,
                    src,
                    description,
                    width,
                    height
                FROM images WHERE id = $1
                "#,
                image_id
            )
            .fetch_optional(&self.client)
            .await
        ))
    }

    pub async fn delete_image_by_id(&self, id: Uuid) -> Result<(), ChonkitError> {
        map_err!(
            sqlx::query!("DELETE FROM images WHERE id = $1", id)
                .execute(&self.client)
                .await
        );
        Ok(())
    }
}
