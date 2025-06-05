use super::Atomic;
use crate::{
    core::{
        model::{
            image::{ImageModel, InsertImage},
            List,
        },
        repo::Repository,
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
        tx: Option<&mut <Self as Atomic>::Tx>,
    ) -> Result<ImageModel, ChonkitError>
    where
        Self: Atomic,
    {
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
            "#,
            insert.document_id,
            insert.page_number as i32,
            insert.image_number as i32,
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
        document_id: Uuid,
        image_path: &str,
        description: Option<&str>,
    ) -> Result<(), ChonkitError> {
        if sqlx::query!("SELECT COUNT(*) FROM documents WHERE id = $1", document_id,)
            .fetch_one(&self.client)
            .await
            .unwrap()
            .count
            .is_some_and(|count| count == 0)
        {
            return err!(DoesNotExist, "Document does not exist");
        }

        let count = map_err!(
            sqlx::query!(
                "UPDATE images SET description = $1 WHERE document_id = $2 AND path = $3",
                description,
                document_id,
                image_path
            )
            .execute(&self.client)
            .await
        );

        if count.rows_affected() == 0 {
            return err!(DoesNotExist, "Image does not exist");
        }

        Ok(())
    }

    pub async fn list_document_images(
        &self,
        document_id: Uuid,
        src: &str,
    ) -> Result<List<ImageModel>, ChonkitError> {
        let total = map_err!(sqlx::query!(
            "SELECT COUNT(*) FROM images WHERE document_id = $1 AND src = $2",
            document_id,
            src
        )
        .fetch_one(&self.client)
        .await
        .map(|row| row.count.map(|c| c as usize)));

        let images = map_err!(
            sqlx::query_as!(
                ImageModel,
                r#"
                SELECT
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
        );

        Ok(List::new(total, images))
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

    pub async fn list_document_image_paths(
        &self,
        document_id: Uuid,
        src: &str,
    ) -> Result<Vec<String>, ChonkitError> {
        Ok(map_err!(sqlx::query!(
            "SELECT path FROM images WHERE document_id = $1 AND src = $2",
            document_id,
            src
        )
        .fetch_all(&self.client)
        .await
        .map(|rows| rows.into_iter().map(|row| row.path).collect())))
    }
}
