#[macro_export]
macro_rules! unwrap_option {
    ( $e:expr ) => {
        match $e {
            Some(x) => x,
            None => return ControlFlow::Break("Failed to execute command".to_string()),
        }
    };
    ( $e:expr, $msg:expr ) => {
        match $e {
            Some(x) => x,
            None => return ControlFlow::Break($msg),
        }
    }
}

#[macro_export]
macro_rules! unwrap_result {
    ( $e:expr ) => {
        match $e {
            Ok(x) => x,
            Err(_) => return ControlFlow::Break("Failed to execute command".to_string()),
        }
    };
    ( $e:expr, $msg:expr ) => {
        match $e {
            Ok(x) => x,
            Err(_) => return ControlFlow::Break($msg),
        }
    }
}

#[macro_export]
macro_rules! unwrap_or_none {
    ( $e:expr ) => {
        match $e {
            Ok(x) => x,
            Err(e) => {
                log::error!("{}", e);
                return Option::None
            },
        }
    }
}
