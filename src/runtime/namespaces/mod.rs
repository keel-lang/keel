use crate::interpreter::{Interpreter, Namespace};

mod agent;
mod ai;
mod asynchronous;
mod cache;
mod control;
mod crypto;
mod csv;
mod db;
mod email;
mod env;
mod file;
mod http;
mod io;
mod json;
mod log;
mod math;
pub(crate) mod memory;
mod random;
mod schedule;
mod search;
mod shell;
mod time;
pub(crate) mod uuid;

pub(crate) fn install(interp: &mut Interpreter) {
    for namespace in namespaces() {
        interp.register_namespace(namespace);
    }
}

fn namespaces() -> [Namespace; 23] {
    [
        io::namespace(),
        schedule::namespace(),
        ai::namespace(),
        email::namespace(),
        env::namespace(),
        memory::namespace(),
        log::namespace(),
        agent::namespace(),
        control::namespace(),
        asynchronous::namespace(),
        http::namespace(),
        search::namespace(),
        db::namespace(),
        time::namespace(),
        file::namespace(),
        json::namespace(),
        cache::namespace(),
        random::namespace(),
        uuid::namespace(),
        crypto::namespace(),
        math::namespace(),
        shell::namespace(),
        csv::namespace(),
    ]
}
