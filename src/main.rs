#[macro_use]
extern crate log;
extern crate iron;

extern crate codechain_rpc as crpc;
extern crate jsonrpc_core;
extern crate primitives as cprimitives;
extern crate rand;
extern crate serde;
#[macro_use]
extern crate serde_derive;
#[macro_use]
extern crate serde_json;
extern crate ws;

#[macro_use]
mod logger;
mod agent;
mod common_rpc_types;
mod db;
mod event_propagator;
mod frontend;
mod jsonrpc;
mod router;
mod rpc;

use std::cell::Cell;
use std::fmt;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::Mutex;
use std::thread;

use iron::prelude::*;
use iron::status;
use ws::listen;

use self::agent::SendAgentRPC;
use self::event_propagator::EventPropagator;
use self::logger::init as logger_init;
use self::router::Router;

fn main() {
    logger_init().expect("Logger should be initialized");

    let frontend_service_sender = frontend::Service::run_thread();
    let event_propagater = Box::new(EventPropagator::new(frontend_service_sender.clone()));
    let db_service_sender = db::Service::run_thread(event_propagater);
    let agent_service_sender = agent::Service::run_thread(db_service_sender.clone());
    let agent_service_for_frontend = agent_service_sender.clone();
    let web_handler = WebHandler::new(agent_service_sender.clone());

    let frontend_join = thread::Builder::new()
        .name("frontend listen".to_string())
        .spawn(move || {
            let count = Rc::new(Cell::new(0));
            let mut frontend_router = Arc::new(Router::new());
            frontend::add_routing(Arc::get_mut(&mut frontend_router).unwrap());
            let frontend_context = frontend::Context {
                agent_service: agent_service_for_frontend,
                db_service: db_service_sender.clone(),
            };
            listen("127.0.0.1:3012", move |out| frontend::WebSocketHandler {
                out,
                count: count.clone(),
                context: frontend_context.clone(),
                router: frontend_router.clone(),
                frontend_service: frontend_service_sender.clone(),
            }).unwrap();
        })
        .expect("Should success listening frontend");

    let agent_join = thread::Builder::new()
        .name("agent listen".to_string())
        .spawn(move || {
            let count = Rc::new(Cell::new(0));
            listen("0.0.0.0:4012", |out| {
                agent::WebSocketHandler::new(out, count.clone(), agent_service_sender.clone())
            }).unwrap();
        })
        .expect("Should success listening agent");

    let webserver_join = thread::Builder::new()
        .name("webserver".to_string())
        .spawn(move || {
            let _server = Iron::new(web_handler).http("0.0.0.0:5012").unwrap();
            cinfo!("Webserver listening on 5012");
        })
        .expect("Should success open webserver");

    frontend_join.join().expect("Join frotend listner");
    agent_join.join().expect("Join agent listner");
    webserver_join.join().expect("Join webserver");
}

struct WebHandler {
    agent_service_sender: Mutex<agent::ServiceSender>,
}

impl WebHandler {
    fn new(agent_service_sender: agent::ServiceSender) -> Self {
        Self {
            agent_service_sender: Mutex::new(agent_service_sender),
        }
    }
}

impl iron::Handler for WebHandler {
    fn handle(&self, req: &mut iron::Request) -> IronResult<iron::Response> {
        let paths = req.url.path();
        if paths.len() != 2 {
            cwarn!("Invalid web request {}", req.url);
            return Ok(Response::with(status::NotFound))
        }

        if paths.get(0).expect("Already checked") != &"log" {
            cwarn!("Invalid web request {}", req.url);
            return Ok(Response::with(status::NotFound))
        }

        let node_name = *paths.get(1).expect("Already checked");
        ctrace!("Get log for agent-{}", node_name);

        let agent = self
            .agent_service_sender
            .lock()
            .expect("Should success get lock")
            .get_agent(node_name.to_string())
            .ok_or_else(|| iron::IronError::new(WebError::new("Not Found"), status::NotFound))?;

        let log =
            agent.shell_get_codechain_log().map_err(|err| iron::IronError::new(err, status::InternalServerError))?;

        use iron::mime;
        let content_type = "text/plain".parse::<mime::Mime>().unwrap();
        Ok(Response::with((content_type, status::Ok, log)))
    }
}

#[derive(Debug)]
struct WebError {
    value: String,
}

impl WebError {
    fn new(s: &str) -> Self {
        WebError {
            value: s.to_string(),
        }
    }
}

impl fmt::Display for WebError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::Debug::fmt(self, f)
    }
}

impl std::error::Error for WebError {}
