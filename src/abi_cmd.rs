use clap::{App, Arg, ArgMatches, SubCommand};
use serde_json::{self, Value};
use std::fs;
use std::str;

use crate::abi::{decode_input, decode_logs, decode_params, encode_input, encode_params};

/// Ethereum abi sub command
pub fn sub_command(name: &'static str) -> App<'static, 'static> {
    let param_arg = Arg::with_name("param")
        .long("param")
        .takes_value(true)
        .multiple(true)
        .allow_hyphen_values(true)
        .number_of_values(2)
        .help("Function parameters");
    let no_lenient_flag = Arg::with_name("no-lenient")
        .long("no-lenient")
        .help("Don't allow short representation of input params");
    let file_arg = Arg::with_name("file")
        .long("file")
        .takes_value(true)
        .required(true)
        .help("ABI json file path");

    App::new(name)
        .about("ABI operation, encode parameter, generate code based on abi and parameters")
        .subcommand(
            SubCommand::with_name("encode")
                .subcommand(
                    SubCommand::with_name("function")
                        .arg(file_arg.clone())
                        .arg(
                            Arg::with_name("name")
                                .long("name")
                                .takes_value(true)
                                .required(true)
                                .help("Function name"),
                        )
                        .arg(param_arg.clone().number_of_values(1).value_name("value"))
                        .arg(no_lenient_flag.clone()),
                )
                .subcommand(
                    SubCommand::with_name("params")
                        .arg(param_arg.clone().value_names(&["type", "value"]))
                        .arg(no_lenient_flag.clone()),
                )
                .subcommand(
                    SubCommand::with_name("constructor")
                        .arg(file_arg.clone())
                        .arg(
                            Arg::with_name("code")
                                .long("code")
                                .takes_value(true)
                                .default_value("")
                                .help("Contract bin code"),
                        )
                        .arg(no_lenient_flag)
                        .arg(param_arg.clone().number_of_values(1).value_name("value")),
                ),
        )
        .subcommand(
            SubCommand::with_name("decode")
                .subcommand(
                    SubCommand::with_name("params")
                        .arg(
                            Arg::with_name("type")
                                .long("type")
                                .takes_value(true)
                                .multiple(true)
                                .help("Decode types"),
                        )
                        .arg(
                            Arg::with_name("data")
                                .long("data")
                                .takes_value(true)
                                .help("Decode data"),
                        ),
                )
                .subcommand(
                    SubCommand::with_name("function")
                        .arg(file_arg.clone())
                        .arg(
                            Arg::with_name("name")
                                .long("name")
                                .takes_value(true)
                                .required(true)
                                .help("Function name"),
                        )
                        .arg(
                            Arg::with_name("data")
                                .long("data")
                                .required(true)
                                .takes_value(true)
                                .help("Decode data"),
                        ),
                )
                .subcommand(
                    SubCommand::with_name("log")
                        .arg(file_arg.clone())
                        .arg(
                            Arg::with_name("event")
                                .long("event")
                                .takes_value(true)
                                .required(true)
                                .help("Event name"),
                        )
                        .arg(param_arg.clone().number_of_values(1).value_name("topic"))
                        .arg(
                            Arg::with_name("data")
                                .long("data")
                                .required(true)
                                .takes_value(true)
                                .help("Decode data"),
                        ),
                ),
        )
}

pub fn process(sub_matches: &ArgMatches) -> Result<(), String> {
    match sub_matches.subcommand() {
        ("encode", Some(em)) => match em.subcommand() {
            ("function", Some(m)) => {
                let file = m.value_of("file").unwrap();
                let abi = fs::read(file).map_err(|err| err.to_string())?;
                let name = m.value_of("name").unwrap();
                let lenient = !m.is_present("no-lenient");
                let values: Vec<String> = match m.values_of("param") {
                    None => Vec::new(),
                    Some(param) => param.map(|s| s.to_owned()).collect::<Vec<String>>(),
                };
                let output = encode_input(&abi, name, &values, lenient, false)?;
                println!("{}", output);
            }
            ("params", Some(m)) => {
                let lenient = !m.is_present("no-lenient");
                let mut types: Vec<String> = Vec::new();
                let mut values: Vec<String> = Vec::new();
                let mut param_iter = m
                    .values_of("param")
                    .ok_or_else(|| "Please give at least one parameter.".to_string())?
                    .peekable();
                while param_iter.peek().is_some() {
                    types.push(param_iter.next().unwrap().to_owned());
                    values.push(param_iter.next().unwrap().to_owned());
                }
                let output = encode_params(&types, &values, lenient)?;
                println!("{}", output);
            }
            ("constructor", Some(m)) => {
                let file = m.value_of("file").unwrap();
                let abi = fs::read(file).map_err(|err| err.to_string())?;
                let code = m.value_of("code").unwrap();
                let lenient = !m.is_present("no-lenient");
                let values: Vec<String> = match m.values_of("param") {
                    None => Vec::new(),
                    Some(param) => param.map(|s| s.to_owned()).collect::<Vec<String>>(),
                };
                let output = encode_input(&abi, code, &values, lenient, true)?;
                println!("{}", output);
            }
            _ => {
                return Err(em.usage().to_owned());
            }
        },
        ("decode", Some(em)) => match em.subcommand() {
            ("params", Some(m)) => {
                let types: Vec<String> = m
                    .values_of("type")
                    .ok_or_else(|| "Please give at least one parameter.".to_string())?
                    .map(|value| value.to_owned())
                    .collect();
                let data = remove_0x(m.value_of("data").unwrap());
                let output = decode_params(&types, data)?
                    .iter()
                    .map(|value| serde_json::from_str(value).unwrap())
                    .collect();
                println!(
                    "{}",
                    serde_json::to_string_pretty(&Value::Array(output))
                        .map_err(|err| err.to_string())?
                );
            }
            ("function", Some(m)) => {
                let file = m.value_of("file").unwrap();
                let abi = fs::read(file).map_err(|err| err.to_string())?;
                let name = m.value_of("name").unwrap();
                let values = m.value_of("data").unwrap();
                let output = decode_input(&abi, name, values)?
                    .iter()
                    .map(|value| serde_json::from_str(value).unwrap())
                    .collect();
                println!(
                    "{}",
                    serde_json::to_string_pretty(&Value::Array(output))
                        .map_err(|err| err.to_string())?
                );
            }
            ("log", Some(m)) => {
                let file = m.value_of("file").unwrap();
                let abi = fs::read(file).map_err(|err| err.to_string())?;
                let event = m.value_of("event").unwrap();
                let topic: Vec<String> = match m.values_of("param") {
                    None => Vec::new(),
                    Some(param) => param.map(|s| s.to_owned()).collect::<Vec<String>>(),
                };
                let data = m.value_of("data").unwrap();
                let output = decode_logs(&abi, event, &topic, data)?
                    .iter()
                    .map(|value| serde_json::from_str(value).unwrap())
                    .collect();
                println!(
                    "{}",
                    serde_json::to_string_pretty(&Value::Array(output))
                        .map_err(|err| err.to_string())?
                );
            }
            _ => {
                return Err(em.usage().to_owned());
            }
        },
        _ => {
            return Err(sub_matches.usage().to_owned());
        }
    }
    Ok(())
}

fn remove_0x(hex: &str) -> &str {
    if hex.len() >= 2 {
        let tmp = hex.as_bytes();
        if tmp[..2] == b"0x"[..] || tmp[..2] == b"0X"[..] {
            return str::from_utf8(&tmp[2..]).unwrap();
        }
    }
    hex
}
