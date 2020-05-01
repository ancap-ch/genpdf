#![warn(unused_extern_crates)]

// TODO:
// depends on ttf-linux-libertine package (Linux Libertine O font)
// depends on fonts-tlwg package (for Norasi font)
// depends on texlive-langchinese package (FandolFang-Regular font)
// depends on otf-ipafont

#[macro_use]
extern crate serde_derive;

extern crate failure;
// extern crate log;

extern crate genpdf;

extern crate actix_web;
extern crate env_logger;
extern crate serde_json;

#[macro_use]
mod macros;

mod web;

use failure::Error;

fn main() -> Result<(), Error> {
    env_logger::init().unwrap_or_else(|_| panic!("{}", &fh!()));

    use std::path::PathBuf;
    let consts_path = PathBuf::from("consts.toml");
    let static_path = PathBuf::from("web_server/static/");
    web::run_with(consts_path, static_path).unwrap_or_else(|_| panic!("{}", &fh!()));

    dbg!("exiting pdfgen..");
    Ok(())
}
