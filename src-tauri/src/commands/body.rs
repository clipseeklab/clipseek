//! Typed command bodies that preserve the existing flat IPC argument object.
use serde::de::DeserializeOwned;
use tauri::{
    ipc::{CommandArg, CommandItem, InvokeBody, InvokeError},
    Runtime,
};

pub struct CommandBody<T>(pub T);

impl<'de, R: Runtime, T: DeserializeOwned> CommandArg<'de, R> for CommandBody<T> {
    fn from_command(command: CommandItem<'de, R>) -> Result<Self, InvokeError> {
        let parse = || match command.message.payload() {
            InvokeBody::Json(value) => serde_json::from_value(value.clone())
                .map(Self)
                .map_err(|error| crate::error::AppError::InvalidArgument(error.to_string())),
            InvokeBody::Raw(_) => Err(crate::error::AppError::InvalidArgument(
                "Expected a JSON command body".into(),
            )),
        };
        parse().map_err(InvokeError::from)
    }
}

/// Library identity travels as IPC metadata, leaving existing JSON bodies intact.
pub struct LibraryContext(pub Option<String>);

impl<'de, R: Runtime> CommandArg<'de, R> for LibraryContext {
    fn from_command(command: CommandItem<'de, R>) -> Result<Self, InvokeError> {
        command
            .message
            .headers()
            .get("x-clipseek-library")
            .map(|value| {
                value.to_str().map(str::to_owned).map_err(|_| {
                    InvokeError::from(crate::error::AppError::InvalidArgument(
                        "Invalid library context".into(),
                    ))
                })
            })
            .transpose()
            .map(Self)
    }
}
