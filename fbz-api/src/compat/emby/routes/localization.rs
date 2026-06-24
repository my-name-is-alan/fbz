use axum::{
    Json,
    extract::State,
    http::{HeaderMap, Uri},
};
use serde::Serialize;

use crate::{error::AppError, state::AppState};

use super::access::authenticate_request_user;

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct CountryInfoDto {
    pub name: String,
    pub display_name: String,
    pub english_name: String,
    #[serde(rename = "TwoLetterISORegionName")]
    pub two_letter_iso_region_name: String,
    #[serde(rename = "ThreeLetterISORegionName")]
    pub three_letter_iso_region_name: String,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct CultureDto {
    pub name: String,
    pub display_name: String,
    #[serde(rename = "TwoLetterISOLanguageName")]
    pub two_letter_iso_language_name: String,
    #[serde(rename = "ThreeLetterISOLanguageName")]
    pub three_letter_iso_language_name: String,
    #[serde(rename = "ThreeLetterISOLanguageNames")]
    pub three_letter_iso_language_names: Vec<String>,
    #[serde(rename = "TwoLetterISOLanguageNames")]
    pub two_letter_iso_language_names: Vec<String>,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct LocalizationOptionDto {
    pub name: String,
    pub value: String,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct ParentalRatingDto {
    pub name: String,
    pub value: i32,
}

pub async fn countries(
    State(state): State<AppState>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<Vec<CountryInfoDto>>, AppError> {
    authenticate_request_user(&state, &headers, &uri).await?;

    Ok(Json(country_items()))
}

pub async fn cultures(
    State(state): State<AppState>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<Vec<CultureDto>>, AppError> {
    authenticate_request_user(&state, &headers, &uri).await?;

    Ok(Json(culture_items()))
}

pub async fn options(
    State(state): State<AppState>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<Vec<LocalizationOptionDto>>, AppError> {
    authenticate_request_user(&state, &headers, &uri).await?;

    Ok(Json(localization_options()))
}

pub async fn parental_ratings(
    State(state): State<AppState>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<Vec<ParentalRatingDto>>, AppError> {
    authenticate_request_user(&state, &headers, &uri).await?;

    Ok(Json(parental_rating_items()))
}

fn country_items() -> Vec<CountryInfoDto> {
    vec![
        CountryInfoDto {
            name: "US".to_owned(),
            display_name: "United States".to_owned(),
            english_name: "United States".to_owned(),
            two_letter_iso_region_name: "US".to_owned(),
            three_letter_iso_region_name: "USA".to_owned(),
        },
        CountryInfoDto {
            name: "CN".to_owned(),
            display_name: "China".to_owned(),
            english_name: "China".to_owned(),
            two_letter_iso_region_name: "CN".to_owned(),
            three_letter_iso_region_name: "CHN".to_owned(),
        },
    ]
}

fn culture_items() -> Vec<CultureDto> {
    vec![
        CultureDto {
            name: "en-US".to_owned(),
            display_name: "English (United States)".to_owned(),
            two_letter_iso_language_name: "en".to_owned(),
            three_letter_iso_language_name: "eng".to_owned(),
            three_letter_iso_language_names: vec!["eng".to_owned()],
            two_letter_iso_language_names: vec!["en".to_owned()],
        },
        CultureDto {
            name: "zh-CN".to_owned(),
            display_name: "Chinese (Simplified, China)".to_owned(),
            two_letter_iso_language_name: "zh".to_owned(),
            three_letter_iso_language_name: "zho".to_owned(),
            three_letter_iso_language_names: vec!["zho".to_owned(), "chi".to_owned()],
            two_letter_iso_language_names: vec!["zh".to_owned()],
        },
    ]
}

fn localization_options() -> Vec<LocalizationOptionDto> {
    culture_items()
        .into_iter()
        .map(|culture| LocalizationOptionDto {
            name: culture.display_name,
            value: culture.name,
        })
        .collect()
}

fn parental_rating_items() -> Vec<ParentalRatingDto> {
    vec![
        ParentalRatingDto {
            name: "G".to_owned(),
            value: 1,
        },
        ParentalRatingDto {
            name: "PG".to_owned(),
            value: 5,
        },
        ParentalRatingDto {
            name: "PG-13".to_owned(),
            value: 8,
        },
        ParentalRatingDto {
            name: "R".to_owned(),
            value: 9,
        },
        ParentalRatingDto {
            name: "NC-17".to_owned(),
            value: 10,
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn localization_options_are_derived_from_supported_cultures() {
        let cultures = culture_items();
        let options = localization_options();

        assert_eq!(options.len(), cultures.len());
        assert_eq!(options[0].name, cultures[0].display_name);
        assert_eq!(options[0].value, cultures[0].name);
    }

    #[test]
    fn localization_country_and_culture_dtos_preserve_iso_acronym_keys() {
        let country = serde_json::to_value(&country_items()[0]).unwrap();
        assert_eq!(country["TwoLetterISORegionName"], "US");
        assert_eq!(country["ThreeLetterISORegionName"], "USA");

        let culture = serde_json::to_value(&culture_items()[0]).unwrap();
        assert_eq!(culture["TwoLetterISOLanguageName"], "en");
        assert_eq!(culture["ThreeLetterISOLanguageName"], "eng");
        assert_eq!(culture["TwoLetterISOLanguageNames"][0], "en");
        assert_eq!(culture["ThreeLetterISOLanguageNames"][0], "eng");
    }

    #[test]
    fn parental_rating_items_include_common_us_ratings_in_order() {
        let ratings = parental_rating_items();

        assert_eq!(
            ratings.first().map(|rating| rating.name.as_str()),
            Some("G")
        );
        assert_eq!(
            ratings.last().map(|rating| rating.name.as_str()),
            Some("NC-17")
        );
        assert!(
            ratings
                .windows(2)
                .all(|pair| pair[0].value <= pair[1].value)
        );
    }
}
