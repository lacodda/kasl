use chrono::prelude::Local;
use reqwest::{
    header::{HeaderMap, HeaderValue, COOKIE},
    multipart, Client, StatusCode,
};
use std::error::Error;

pub struct Http {
    client: Client,
}

impl Http {
    pub fn new() -> Self {
        Self { client: Client::new() }
    }

    pub async fn send(&self, url: &str, session_id: &str, data: String) -> Result<StatusCode, Box<dyn Error>> {
        let date = Local::now().format("%Y-%m-%d").to_string();
        let form = multipart::Form::new()
            .text("date", date)
            .text("tasks", data)
            .text("comment", "")
            .text("day_type", "1")
            .text("duty", "0")
            .text("only_save", "0");

        let mut headers = HeaderMap::new();
        headers.insert(COOKIE, HeaderValue::from_str(&format!("PORTALSESSID={}", session_id))?);

        let res = self.client.post(url).headers(headers).multipart(form).send().await?;

        // println!("Status: {}", res.status());
        // println!("Headers:\n{:#?}", res.headers());
        // let body = res.text().await?;
        // println!("Body:\n{:#?}", body);
        Ok(res.status())
    }
}
