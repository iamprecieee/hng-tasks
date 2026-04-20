use serde::Deserialize;

#[derive(Debug, Default, Deserialize)]
pub enum AgeGroup {
    #[default]
    Child,
    Teenager,
    Adult,
    Senior,
}

impl AgeGroup {
    pub fn classify(age: u8) -> Self {
        match age {
            0..=12 => Self::Child,
            13..=19 => Self::Teenager,
            20..=59 => Self::Adult,
            60.. => Self::Senior,
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct AgifyResponse {
    pub age: Option<u8>,
    #[serde(skip)]
    pub age_group: AgeGroup,
}
