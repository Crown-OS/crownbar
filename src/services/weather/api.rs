//! The two services behind the weather widget, and nothing else.
//!
//! Both are plain HTTPS GETs returning JSON. They are the only outbound
//! network calls the bar makes, so they are kept together where they can be
//! read at a glance: what is asked for, and what leaves the machine.
//!
//! * **ipapi.co** resolves the machine's public address to a rough position.
//!   The request carries no body — the address it reads is the one the
//!   connection already has. It is made once per run and cached.
//! * **Open-Meteo** answers the forecast. It needs no key and no account, and
//!   is sent nothing but the coordinates.

use std::time::Duration;

use serde::Deserialize;

use crate::services::weather::condition::Condition;

/// Long enough to survive a slow link, short enough that a failure does not
/// hold a refresh open until the next one is due.
const TIMEOUT: Duration = Duration::from_secs(10);
const LOCATE: &str = "https://ipapi.co/json/";
const FORECAST: &str = "https://api.open-meteo.com/v1/forecast";
/// How many days the panel lists, today included.
pub const DAYS: usize = 5;

#[derive(Clone, Debug, PartialEq)]
pub struct Place {
    pub latitude: f64,
    pub longitude: f64,
    pub name: String,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Current {
    pub condition: Condition,
    /// Whether the sun is up *there*, which is what picks the night pose.
    pub night: bool,
    pub celsius: f32,
    pub feels_like: f32,
    pub humidity: u8,
    pub wind_kph: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Outlook {
    /// "Today", then a weekday name.
    pub day: String,
    pub condition: Condition,
    pub high: f32,
    pub low: f32,
}

pub fn client() -> reqwest::Result<reqwest::Client> {
    reqwest::Client::builder()
        .timeout(TIMEOUT)
        // Identifying the caller is the courteous thing to do with a free,
        // unauthenticated service, and it is what lets the operator tell a
        // desktop widget apart from a scraper.
        .user_agent(concat!("crownbar/", env!("CARGO_PKG_VERSION")))
        .build()
}

pub async fn locate(client: &reqwest::Client) -> anyhow::Result<Place> {
    #[derive(Deserialize)]
    struct Response {
        latitude: f64,
        longitude: f64,
        city: Option<String>,
        region: Option<String>,
    }

    let response: Response = client.get(LOCATE).send().await?.json().await?;
    Ok(Place {
        latitude: response.latitude,
        longitude: response.longitude,
        name: response
            .city
            .or(response.region)
            .unwrap_or_else(|| "Here".to_string()),
    })
}

pub async fn forecast(
    client: &reqwest::Client,
    place: &Place,
) -> anyhow::Result<(Current, Vec<Outlook>)> {
    #[derive(Deserialize)]
    struct Response {
        current: CurrentBlock,
        daily: DailyBlock,
    }

    #[derive(Deserialize)]
    struct CurrentBlock {
        temperature_2m: f32,
        apparent_temperature: f32,
        relative_humidity_2m: f32,
        wind_speed_10m: f32,
        weather_code: u8,
        is_day: u8,
    }

    #[derive(Deserialize)]
    struct DailyBlock {
        time: Vec<String>,
        weather_code: Vec<u8>,
        temperature_2m_max: Vec<f32>,
        temperature_2m_min: Vec<f32>,
    }

    let response: Response = client
        .get(FORECAST)
        .query(&[
            ("latitude", place.latitude.to_string()),
            ("longitude", place.longitude.to_string()),
            (
                "current",
                "temperature_2m,apparent_temperature,relative_humidity_2m,\
                 wind_speed_10m,weather_code,is_day"
                    .to_string(),
            ),
            (
                "daily",
                "weather_code,temperature_2m_max,temperature_2m_min".to_string(),
            ),
            ("timezone", "auto".to_string()),
            ("forecast_days", DAYS.to_string()),
        ])
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;

    let current = Current {
        condition: Condition::from_wmo(response.current.weather_code),
        night: response.current.is_day == 0,
        celsius: response.current.temperature_2m,
        feels_like: response.current.apparent_temperature,
        humidity: response.current.relative_humidity_2m.clamp(0.0, 100.0) as u8,
        wind_kph: response.current.wind_speed_10m,
    };

    let daily = &response.daily;
    let outlook = (0..daily.time.len().min(DAYS))
        .filter_map(|i| {
            Some(Outlook {
                day: weekday(daily.time.get(i)?, i),
                condition: Condition::from_wmo(*daily.weather_code.get(i)?),
                high: *daily.temperature_2m_max.get(i)?,
                low: *daily.temperature_2m_min.get(i)?,
            })
        })
        .collect();

    Ok((current, outlook))
}

/// `2026-09-23` to the name the panel lists it under.
///
/// The dates come back in the *forecast's* timezone, so they are read as plain
/// dates rather than instants — turning them into local time would put a
/// traveller's "Today" on the wrong row.
fn weekday(date: &str, index: usize) -> String {
    use chrono::NaiveDate;

    if index == 0 {
        return "Today".to_string();
    }
    match NaiveDate::parse_from_str(date, "%Y-%m-%d") {
        Ok(date) => date.format("%A").to_string(),
        Err(_) => date.to_string(),
    }
}
