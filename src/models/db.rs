use futures::stream::TryStreamExt;
use mongodb::{
    Collection, Database, IndexModel, bson,
    error::{ErrorKind, WriteFailure},
    options::IndexOptions,
};
use serde::{Deserialize, Serialize};

use crate::{
    errors::{AppError, Result},
    models::profile::{SortBy, SortOrder},
};

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Profile {
    pub id: String,
    pub name: String,
    pub gender: String,
    pub gender_probability: f64,
    pub age: u8,
    pub age_group: String,
    pub country_id: String,
    pub country_name: String,
    pub country_probability: f64,
    pub created_at: String,
}

#[derive(Debug, Default)]
pub struct ProfileFilters {
    pub gender: Option<String>,
    pub country_id: Option<String>,
    pub age_group: Option<String>,
    pub min_age: Option<u8>,
    pub max_age: Option<u8>,
    pub min_gender_probability: Option<f64>,
    pub min_country_probability: Option<f64>,
}

#[derive(Clone)]
pub struct ProfileRepo {
    collection: Collection<Profile>,
}

impl std::fmt::Debug for ProfileRepo {
    fn fmt(&self, func: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        func.debug_struct("ProfileRepo").finish()
    }
}

impl ProfileRepo {
    pub fn new(db: &Database) -> Self {
        Self {
            collection: db.collection("profiles"),
        }
    }

    pub async fn create_indexes(&self) -> Result<()> {
        let name_index = IndexModel::builder()
            .keys(bson::doc! { "name": 1 })
            .options(
                IndexOptions::builder()
                    .unique(true)
                    .name("idx_name_unique".to_string())
                    .build(),
            )
            .build();

        let id_index = IndexModel::builder()
            .keys(bson::doc! { "id": 1 })
            .options(
                IndexOptions::builder()
                    .unique(true)
                    .name("idx_id_unique".to_string())
                    .build(),
            )
            .build();

        let filter_index = IndexModel::builder()
            .keys(bson::doc! { "country_id": 1, "gender": 1, "age_group": 1 })
            .options(
                IndexOptions::builder()
                    .name("idx_filters".to_string())
                    .build(),
            )
            .build();

        let age_index = IndexModel::builder()
            .keys(bson::doc! { "age": 1 })
            .options(IndexOptions::builder().name("idx_age".to_string()).build())
            .build();

        let created_at_index = IndexModel::builder()
            .keys(bson::doc! { "created_at": 1 })
            .options(
                IndexOptions::builder()
                    .name("idx_created_at".to_string())
                    .build(),
            )
            .build();

        let prob_index = IndexModel::builder()
            .keys(bson::doc! { "gender_probability": 1 })
            .options(
                IndexOptions::builder()
                    .name("idx_gender_prob".to_string())
                    .build(),
            )
            .build();

        self.collection
            .create_indexes(vec![
                name_index,
                id_index,
                filter_index,
                age_index,
                created_at_index,
                prob_index,
            ])
            .await
            .map_err(|e| {
                AppError::ServiceUnavailable(format!("Failed to create indexes: {}", e))
            })?;

        tracing::info!("Database indexes verified");
        Ok(())
    }

    pub async fn find_by_name(&self, name: &str) -> Result<Option<Profile>> {
        self.collection
            .find_one(bson::doc! { "name": name })
            .await
            .map_err(|e| AppError::ServiceUnavailable(format!("DB Search Error: {}", e)))
    }

    pub async fn find_by_id(&self, id: &str) -> Result<Option<Profile>> {
        self.collection
            .find_one(bson::doc! { "id": id })
            .await
            .map_err(|e| AppError::ServiceUnavailable(format!("DB Search Error: {}", e)))
    }

    pub async fn delete_by_id(&self, id: &str) -> Result<bool> {
        let result = self
            .collection
            .delete_one(bson::doc! { "id": id })
            .await
            .map_err(|e| AppError::ServiceUnavailable(format!("DB Delete Error: {}", e)))?;
        Ok(result.deleted_count > 0)
    }

    pub async fn find_paginated(
        &self,
        filters: ProfileFilters,
        sort_by: SortBy,
        order: SortOrder,
        page: u32,
        limit: u32,
    ) -> Result<(Vec<Profile>, u64)> {
        let mut filter_doc = bson::doc! {};

        if let Some(gender) = filters.gender {
            filter_doc.insert(
                "gender",
                bson::doc! { "$regex": format!("^{}$", gender), "$options": "i" },
            );
        }

        if let Some(country) = filters.country_id {
            filter_doc.insert(
                "country_id",
                bson::doc! { "$regex": format!("^{}$", country), "$options": "i" },
            );
        }

        if let Some(age) = filters.age_group {
            filter_doc.insert(
                "age_group",
                bson::doc! { "$regex": format!("^{}$", age), "$options": "i" },
            );
        }

        let mut age_doc = bson::doc! {};

        if let Some(min_age) = filters.min_age {
            age_doc.insert("$gte", min_age as i32);
        }

        if let Some(max_age) = filters.max_age {
            age_doc.insert("$lte", max_age as i32);
        }

        if !age_doc.is_empty() {
            filter_doc.insert("age", age_doc);
        }

        if let Some(min_gender_prob) = filters.min_gender_probability {
            filter_doc.insert("gender_probability", bson::doc! { "$gte": min_gender_prob });
        }

        if let Some(min_country_prob) = filters.min_country_probability {
            filter_doc.insert(
                "country_probability",
                bson::doc! { "$gte": min_country_prob },
            );
        }

        let sort_field = match sort_by {
            SortBy::Age => "age",
            SortBy::CreatedAt => "created_at",
            SortBy::GenderProbability => "gender_probability",
        };

        let sort_direction = match order {
            SortOrder::Asc => 1,
            SortOrder::Desc => -1,
        };

        let sort_doc = bson::doc! { sort_field: sort_direction };
        let skip = (page.saturating_sub(1)) * limit;

        let find_options = mongodb::options::FindOptions::builder()
            .sort(sort_doc)
            .skip(skip as u64)
            .limit(limit as i64)
            .build();

        let cursor_future = self
            .collection
            .find(filter_doc.clone())
            .with_options(find_options);

        let count_future = self.collection.count_documents(filter_doc);

        let (cursor_res, count_res) = tokio::join!(cursor_future, count_future);

        let cursor = cursor_res
            .map_err(|e| AppError::ServiceUnavailable(format!("DB Find Error: {}", e)))?;
        let count = count_res
            .map_err(|e| AppError::ServiceUnavailable(format!("DB Count Error: {}", e)))?;

        let profiles: Vec<Profile> = cursor
            .try_collect()
            .await
            .map_err(|e| AppError::ServiceUnavailable(format!("DB Cursor Error: {}", e)))?;

        Ok((profiles, count))
    }

    pub async fn insert_profile(&self, profile: Profile) -> Result<()> {
        match self.collection.insert_one(profile).await {
            Ok(_) => Ok(()),
            Err(e) => {
                if let ErrorKind::Write(WriteFailure::WriteError(ref write_error)) = *e.kind
                    && write_error.code == 11000
                {
                    return Err(AppError::BadRequest(
                        "A profile with this name already exists".to_string(),
                    ));
                }
                Err(AppError::ServiceUnavailable(format!(
                    "DB Insert Error: {}",
                    e
                )))
            }
        }
    }
}
