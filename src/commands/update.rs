use crate::libs::update::Update;
use std::error::Error;

pub async fn cmd() -> Result<(), Box<dyn Error>> {
    Update::new().update_release().await?.update().await?;

    Ok(())
}
