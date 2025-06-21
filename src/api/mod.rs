use crate::libs::{data_storage::DataStorage, secret::Secret};
use std::{
    error::Error,
    fs,
    io::{self, Write},
};

pub mod gitlab;
pub mod jira;
pub mod si;

const MAX_RETRY_COUNT: i32 = 3;

#[allow(async_fn_in_trait)]
pub trait Session {
    async fn login(&self) -> Result<String, Box<dyn Error>>;
    fn set_credentials(&mut self, password: &str) -> Result<(), Box<dyn Error>>;
    fn session_id_file(&self) -> &str;
    fn secret(&self) -> Secret;
    fn retry(&self) -> i32;
    fn inc_retry(&mut self);

    async fn get_session_id(&mut self) -> Result<String, Box<dyn Error>> {
        let session_id_file_path = DataStorage::new().get_path(&self.session_id_file())?;
        let session_id_file_path_str = session_id_file_path.to_str().unwrap();
        if let Ok(session_id) = Self::read_session_id(&session_id_file_path_str) {
            return Ok(session_id);
        } else {
            loop {
                let password: String = match self.retry() > 0 {
                    true => self.secret().prompt()?,
                    false => self.secret().get_or_prompt()?,
                };
                self.set_credentials(&password)?;
                let session_id = self.login().await;
                match session_id {
                    Ok(session_id) => {
                        let _ = Self::write_session_id(&session_id_file_path_str, &session_id);
                        return Ok(session_id);
                    }
                    Err(_) => {
                        if self.retry() < MAX_RETRY_COUNT {
                            self.inc_retry();
                            continue;
                        }
                        break Err(format!("You entered the wrong password {} times!", MAX_RETRY_COUNT).into());
                    }
                }
            }
        }
    }

    fn read_session_id(file_name: &str) -> io::Result<String> {
        fs::read_to_string(file_name)
    }

    fn write_session_id(file_name: &str, session_id: &str) -> io::Result<()> {
        let mut file = fs::OpenOptions::new().write(true).create(true).truncate(true).open(file_name)?;
        file.write_all(session_id.as_bytes())
    }

    fn delete_session_id(&self) -> Result<(), Box<dyn Error>> {
        let session_id_file_path = DataStorage::new().get_path(&self.session_id_file())?;
        fs::remove_file(session_id_file_path)?;
        Ok(())
    }
}
