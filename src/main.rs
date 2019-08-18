
use std::str;

extern crate rand;
extern crate clap;
use clap::{ App, Arg, ArgMatches };

#[macro_use]
extern crate nom;
#[macro_use(position)]
extern crate nom_locate;


#[macro_use]
mod debug;

#[macro_use]
mod parser;

mod abi;
mod ast;
mod misc;
//mod hcode;
mod scope;
mod types;
mod config;
mod session;
mod refinery;
mod binding;
mod typecheck;
mod defs;
mod export;
mod llvm;

use typecheck::TypeChecker;
use config::{ Options, EmitAs };

fn main() {
    let matches =
        App::new("molten")
            .version("0.1")
            .about("a compiler for the molten language")
            .arg(Arg::with_name("INPUT")
                .help("Sets the input file to use")
                .required(true)
                .index(1))
            .arg(Arg::with_name("output")
                .short("o")
                .value_name("OUTPUT")
                .takes_value(true)
                .help("Sets the output file"))
            .arg(Arg::with_name("compile")
                .short("c")
                .conflicts_with("assemble")
                .help("Compiles to an object file"))
            .arg(Arg::with_name("assemble")
                .short("S")
                .conflicts_with("compile")
                .help("Compiles to an assembly file"))
            .arg(Arg::with_name("library")
                .short("l")
                .help("Compiles as a library, without a main function"))
            .arg(Arg::with_name("opt")
                .short("O")
                .takes_value(true)
                .help("Compiles as a library, without a main function"))
            .arg(Arg::with_name("debug")
                .short("d")
                .help("Enables debug logging"))
            .arg(Arg::with_name("no-gc")
                .short("G")
                .long("no-gc")
                .help("Disables garbage collection and uses malloc directly"))
            .get_matches();

    build_options(&matches);

    let input = matches.value_of("INPUT").unwrap();
    let output = matches.value_of("output");
    if matches.occurrences_of("compile") > 0 || matches.occurrences_of("assemble") > 0 {
        compile_file(input, output);
    } else {
        println!("Use the -c flag to compile");
    }
}

fn build_options(matches: &ArgMatches) {
    Options::init();
    Options::as_ref().debug = matches.occurrences_of("debug") > 0;
    Options::as_ref().is_library = matches.occurrences_of("library") > 0;
    Options::as_ref().no_gc = matches.occurrences_of("no-gc") > 0;

    Options::as_ref().format = if matches.occurrences_of("assemble") > 0 {
        EmitAs::LLIR
    } else {
        EmitAs::Obj
    };

    Options::as_ref().optlevel = matches.value_of("opt").map(|s| s.parse::<u32>().unwrap()).unwrap_or(0);
}

fn compile_file(input: &str, output: Option<&str>) {
    let mut session = session::Session::new();
    let source = input.rsplitn(2, '.').collect::<Vec<&str>>()[1];
    session.name = source.replace("/", ".");
    session.target = output.map(|s| String::from(s)).unwrap_or_else(|| String::from(source));

    let builtins = llvm::lib::get_builtins();
    llvm::lib::make_global(&session, &builtins);

    let mut code = session.parse_file(input, false);
    binding::bind_names(&session, session.map.get_global(), &mut code);
    TypeChecker::check(&session, session.map.get_global(), &code);

    if Options::as_ref().debug {
        let global = session.map.get_global();
        println!("\n{:#?}\n", code);
        debug::print_types(&session, global.clone(), &code);
        debug::print_types_scope(&session, global);
    }

    session.resolve_types();

    export::write_exports(&session, session.map.get_global(), format!("{}.dec", session.target).as_str(), &code);

    let transformer = llvm::transform::Transformer::new(&session);
    transformer.initialize();
    transformer.transform_code(session.map.get_global(), &code);
    if Options::as_ref().debug {
        println!("===================");
        println!("{:#?}", &transformer.globals.borrow());
        println!("===================");
    }

    let llvm = llvm::codegen::LLVM::new(&session);
    llvm.initialize();
    llvm::lib::initialize_builtins(&llvm, &transformer, session.map.get_global(), &builtins);
    llvm.build_module(&transformer.globals.borrow());
    llvm.optimize(Options::as_ref().optlevel);
    llvm.print_module();

    match Options::as_ref().format {
        EmitAs::LLIR => llvm.write_module(format!("{}.ll", session.target).as_str()),
        EmitAs::Obj => llvm.write_object_file(format!("{}.o", session.target).as_str()),
    }
}



