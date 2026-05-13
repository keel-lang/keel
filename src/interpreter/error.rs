pub(crate) fn runtime_error(msg: impl Into<String>) -> miette::Report {
    miette::miette!("{}", msg.into())
}
