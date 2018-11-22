extern crate hyper;

use std::io::{self, Write};
use hyper::Client;
use hyper::rt::{self, Future, Stream};

fn main() {
    rt::run(rt::lazy(|| {
        // This is main future that the runtime will execute.
        //
        // The `lazy` is because we don't want any of this executing *right now*,
        // but rather once the runtime has started up all its resources.
        //
        // This is where we will setup our HTTP client requests.
        // still inside rt::run...
        let client = Client::new();

        let uri = "http://httpbin.org/ip".parse().unwrap();

        client
            .get(uri)
            .map(|res| {
                println!("Response: {}", res.status());
            })
            .map_err(|err| {
                println!("Error: {}", err);
            })
    }));
}