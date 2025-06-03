use super::Atomic;
use crate::{
    core::{
        model::{image::ImageModel, List},
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
        document_id: Uuid,
        src: &str,
        path: &str,
        description: Option<&str>,
        tx: Option<&mut <Self as Atomic>::Tx>,
    ) -> Result<ImageModel, ChonkitError>
    where
        Self: Atomic,
    {
        let query = sqlx::query_as!(
            ImageModel,
            r#"INSERT INTO images (path, document_id, src, description)
               VALUES ($1, $2, $3, $4)
               RETURNING path, document_id, src, description
            "#,
            path,
            document_id,
            src,
            description
        );

        match tx {
            Some(tx) => Ok(map_err!(query.fetch_one(&mut **tx).await)),
            None => Ok(map_err!(query.fetch_one(&self.client).await)),
        }
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
                "SELECT path, document_id, src, description FROM images WHERE document_id = $1 AND src = $2",
                document_id,
                src
            )
            .fetch_all(&self.client)
            .await
        );

        Ok(List::new(total, images))
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
