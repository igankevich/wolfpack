pub struct Errors(Vec<String>);

impl Errors {
    pub fn wrap_other(message: impl ToString, other: impl ToString) -> Self {
        Self(vec![message.to_string(), other.to_string()])
    }

    pub fn wrap(message: impl ToString, other: Self) -> Self {
        let mut messages = other.0;
        messages.push(message.to_string());
        Self(messages)
    }
}

//impl std::error::Error for Errors {}
