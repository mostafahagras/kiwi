#![allow(unused)]
use miette::{Diagnostic, NamedSource, SourceSpan};
use thiserror::Error;

/// The container for all configuration errors
#[derive(Error, Debug, Diagnostic)]
#[error("Config validation failed")]
#[diagnostic(code(config::validation_failed))]
pub struct MultiConfigError {
    #[source_code]
    pub src: NamedSource<String>,

    #[related]
    pub errors: Vec<ConfigError>,
}

#[derive(Error, Debug, Diagnostic)]
pub enum ConfigError {
    #[error("TOML Syntax Error")]
    #[diagnostic(code(config::syntax_error))]
    Syntax {
        #[source_code]
        src: NamedSource<String>,
        #[label("{message}")]
        span: SourceSpan,
        message: String,
    },
    #[error("Invalid layout: '{layout}'")]
    #[diagnostic(code(config::invalid_layout))]
    InvalidLayout {
        #[source_code]
        src: NamedSource<String>,
        layout: String,
        #[label("layout not found")]
        span: SourceSpan,
        #[help]
        suggestion: Option<String>,
    },
    #[error("Redundant alias definition")]
    #[diagnostic(code(config::redundant_alias))]
    RedundantAlias {
        // We don't need src here anymore if we provide it in the parent or
        // we can keep it for standalone use.
        #[source_code]
        src: NamedSource<String>,
        alias1: String,
        alias2: String,
        #[label("first defined as '{alias1}'")]
        span1: SourceSpan,
        #[label("'{alias2}' uses the same keys")]
        span2: SourceSpan,
    },
    #[error("Invalid app name: '{name}'")]
    #[diagnostic(code(config::invalid_app_name))]
    InvalidAppName {
        #[source_code]
        src: NamedSource<String>,
        name: String,
        #[label("app names cannot contain path separators or be empty")]
        span: SourceSpan,
        #[help]
        help: String,
    },
    #[error("Unoptimized modifier combination")]
    #[diagnostic(
        code(config::unoptimized_bind),
        severity(warning),
        help(
            "You have an alias '{alias}' for these modifiers. Using it makes your config cleaner."
        )
    )]
    UnoptimizedBind {
        #[source_code]
        src: NamedSource<String>,
        alias: String,
        #[label("use '{alias}' instead of this")]
        span: SourceSpan,
    },
    #[error("Invalid key binding: '{raw}'")]
    #[diagnostic(code(config::invalid_binding))]
    InvalidBinding {
        #[source_code]
        src: NamedSource<String>,
        raw: String,
        #[label("{message}")]
        span: SourceSpan,
        message: String,
    },
    #[error("Unrecognized component in binding: '{raw}'")]
    #[diagnostic(code(config::binding_typo), severity(warning))]
    BindingTypo {
        #[source_code]
        src: NamedSource<String>,
        raw: String,
        typo: String,
        #[label("unrecognized part: '{typo}'")]
        span: SourceSpan,
        #[help]
        suggestion: String, // e.g., "Did you mean 'hyper+escape'?"
    },
    #[error("Undefined app alias: '${alias}'")]
    #[diagnostic(
        code(config::undefined_app_alias),
        help("Add '{alias} = \"...\"' to your [apps] table first.")
    )]
    UndefinedAppAlias {
        #[source_code]
        src: NamedSource<String>,
        alias: String,
        #[label("this app alias is not defined")]
        span: SourceSpan,
    },
    #[error("Missing required field '{field}' in {table_type}")]
    #[diagnostic(code(config::missing_field))]
    MissingField {
        #[source_code]
        src: NamedSource<String>,
        field: String,
        table_type: String,
        #[label("this table needs an '{field}' key")]
        span: SourceSpan,
    },

    #[error("Invalid timeout value")]
    #[diagnostic(
        code(config::invalid_timeout),
        help("Timeout must be a positive number in milliseconds.")
    )]
    InvalidTimeout {
        #[source_code]
        src: NamedSource<String>,
        #[label("expected a number here")]
        span: SourceSpan,
    },
    #[error("Unknown field '{found}' in layer")]
    #[diagnostic(code(config::unknown_layer_field))]
    UnknownField {
        #[source_code]
        src: NamedSource<String>,
        found: String,
        #[label("this field is not recognized")]
        span: SourceSpan,
        #[help]
        help: String, // e.g., "Did you mean 'activate'?"
    },

    #[error("Timeout should be a number")]
    #[diagnostic(code(config::timeout_type_coercion), severity(warning))]
    TimeoutCoercion {
        #[source_code]
        src: NamedSource<String>,
        #[label("parsed '{val}' as {parsed}ms")]
        span: SourceSpan,
        val: String,
        parsed: i64,
        #[help]
        help: String, // "Help: Use a number instead of a string (e.g., timeout = 500)"
    },
}
