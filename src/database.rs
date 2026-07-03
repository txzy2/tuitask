use crate::types::{Status, TODOData};
use directories::ProjectDirs;
use rusqlite::{Connection, Result as RusqliteResult};
use std::error::Error;
use std::fmt;
use std::fs;
use std::path::PathBuf;

#[derive(Debug)]
pub enum DatabaseError {
    ConnectionError(String),
    QueryError(String),
    UpdateError(String),
}

impl fmt::Display for DatabaseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DatabaseError::ConnectionError(msg) => write!(f, "Database connection error: {}", msg),
            DatabaseError::QueryError(msg) => write!(f, "Database query error: {}", msg),
            DatabaseError::UpdateError(msg) => write!(f, "Database update error: {}", msg),
        }
    }
}

impl Error for DatabaseError {}

pub struct DatabaseManager {
    connection: Option<Connection>,
}

impl DatabaseManager {
    fn default_db_path() -> PathBuf {
        if let Some(proj_dirs) = ProjectDirs::from("", "", "tuitask") {
            let data_dir = proj_dirs.data_dir();
            let _ = fs::create_dir_all(data_dir);
            data_dir.join("data.db")
        } else {
            PathBuf::from("data.db")
        }
    }

    pub fn new() -> Result<Self, DatabaseError> {
        let db_path = Self::default_db_path();
        let connection = match Self::open_sqlite_con(&db_path) {
            Ok(conn) => {
                // Create the todos table if it doesn't exist
                if let Err(e) = conn.execute(
                    "CREATE TABLE IF NOT EXISTS todos (
                        id INTEGER PRIMARY KEY AUTOINCREMENT,
                        title TEXT NOT NULL,
                        message TEXT,
                        status TEXT NOT NULL,
                        date TEXT NOT NULL
                    )",
                    [],
                ) {
                    return Err(DatabaseError::ConnectionError(format!(
                        "Error creating table: {}",
                        e
                    )));
                }
                Some(conn)
            }
            Err(e) => {
                return Err(DatabaseError::ConnectionError(format!(
                    "Error opening database: {}",
                    e
                )));
            }
        };

        Ok(DatabaseManager { connection })
    }

    fn open_sqlite_con(db_path: impl AsRef<std::path::Path>) -> RusqliteResult<Connection> {
        Connection::open(db_path)
    }

    pub fn get_connection(&self) -> Option<&Connection> {
        self.connection.as_ref()
    }

    pub fn load_todos(&self) -> Result<Vec<TODOData>, DatabaseError> {
        if let Some(conn) = &self.connection {
            let mut stmt = conn.prepare(
                "
                    SELECT id, title, message, status, date 
                    FROM todos ORDER BY CASE status
                    WHEN 'Active' THEN 1 WHEN 'Todo' THEN 2 WHEN 'Cancelled' THEN 3 WHEN 'Done' THEN 4 ELSE 5 END, id
                ",
            ).map_err(|e| DatabaseError::QueryError(e.to_string()))?;

            let todo_iter = stmt
                .query_map([], |row| {
                    let id: i64 = row.get(0)?;
                    let title: String = row.get(1)?;
                    let message: String = row.get(2)?;
                    let status_str: String = row.get(3)?;
                    let date_str: String = row.get(4)?;

                    let status = match status_str.as_str() {
                        "Todo" => Status::Todo,
                        "Active" => Status::Active,
                        "Done" => Status::Done,
                        "Cancelled" => Status::Cancelled,
                        _ => Status::Todo, // default
                    };

                    let date = chrono::DateTime::parse_from_rfc3339(&date_str)
                        .unwrap_or_else(|_| chrono::Local::now().into())
                        .with_timezone(&chrono::Local);

                    // Convert String to &'static str by leaking the memory (not ideal but needed for the current struct design)
                    Ok(TODOData {
                        id,
                        title: Box::leak(title.into_boxed_str()),
                        message: Box::leak(message.into_boxed_str()),
                        status,
                        date,
                    })
                })
                .map_err(|e| {
                    // Convert rusqlite error to DatabaseError
                    match e {
                        rusqlite::Error::InvalidQuery => {
                            DatabaseError::QueryError("Invalid query".to_string())
                        }
                        rusqlite::Error::InvalidParameterName(_) => {
                            DatabaseError::QueryError("Invalid parameter name".to_string())
                        }
                        _ => DatabaseError::QueryError(e.to_string()),
                    }
                })?;

            let mut items = Vec::new();
            for todo in todo_iter {
                match todo {
                    Ok(todo) => items.push(todo),
                    Err(e) => {
                        // Convert rusqlite error to DatabaseError
                        return Err(match e {
                            rusqlite::Error::InvalidQuery => {
                                DatabaseError::QueryError("Invalid query".to_string())
                            }
                            rusqlite::Error::InvalidParameterName(_) => {
                                DatabaseError::QueryError("Invalid parameter name".to_string())
                            }
                            _ => DatabaseError::QueryError(e.to_string()),
                        });
                    }
                }
            }

            Ok(items)
        } else {
            Err(DatabaseError::ConnectionError(
                "Database connection not available".to_string(),
            ))
        }
    }

    pub fn add_todo(
        &self,
        title: &str,
        message: &str,
        status: Status,
    ) -> Result<i64, DatabaseError> {
        if let Some(conn) = &self.connection {
            let status_str = match status {
                Status::Todo => "Todo",
                Status::Active => "Active",
                Status::Done => "Done",
                Status::Cancelled => "Cancelled",
            };

            let date_str = chrono::Local::now().to_rfc3339();

            conn.execute(
                "INSERT INTO todos (title, message, status, date) VALUES (?1, ?2, ?3, ?4)",
                [title, message, status_str, &date_str],
            )
            .map_err(|e| DatabaseError::UpdateError(e.to_string()))?;

            Ok(conn.last_insert_rowid())
        } else {
            Err(DatabaseError::ConnectionError(
                "Database connection not available".to_string(),
            ))
        }
    }

    pub fn update_todo_status(&self, id: i64, status: Status) -> Result<(), DatabaseError> {
        if let Some(conn) = &self.connection {
            let status_str = match status {
                Status::Todo => "Todo",
                Status::Active => "Active",
                Status::Done => "Done",
                Status::Cancelled => "Cancelled",
            };

            conn.execute(
                "UPDATE todos SET status = ?1 WHERE id = ?2",
                [status_str, &id.to_string()],
            )
            .map_err(|e| DatabaseError::UpdateError(e.to_string()))?;

            Ok(())
        } else {
            Err(DatabaseError::ConnectionError(
                "Database connection not available".to_string(),
            ))
        }
    }

    pub fn delete_todo(&self, id: i64) -> Result<(), DatabaseError> {
        if let Some(conn) = &self.connection {
            conn.execute("DELETE FROM todos WHERE id = ?1", [id])
                .map_err(|e| DatabaseError::UpdateError(e.to_string()))?;
            Ok(())
        } else {
            Err(DatabaseError::ConnectionError(
                "Database connection not available".to_string(),
            ))
        }
    }
}

