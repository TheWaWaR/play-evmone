use ethabi::param_type::{ParamType, Reader};
use ethabi::token::{LenientTokenizer, StrictTokenizer, Token, Tokenizer};
use ethabi::{decode, encode, Contract, Hash};
use ethereum_types::U256;
use hex::{decode as hex_decode, encode as hex_encode};

fn lower_hex(value: U256) -> String {
    format!("{:x}", value)
}
fn completed_lower_hex(value: U256) -> String {
    let len = 256 / 4;
    format!("{:0>width$}", lower_hex(value), width = len)
}

pub fn parse_tokens(params: &[(ParamType, &str)], lenient: bool) -> Result<Vec<Token>, String> {
    params
        .iter()
        .map(|&(ref param, value)| {
            if lenient {
                let type_name = format!("{}", param);
                if type_name.starts_with("uint") && type_name.find(']').is_none() {
                    let y = U256::from_dec_str(value)
                        .map(completed_lower_hex)
                        .map_err(|_| "Can't parse into u256")?;
                    StrictTokenizer::tokenize(param, &y)
                } else if type_name.starts_with("int") && type_name.find(']').is_none() {
                    let x = if value.starts_with('-') {
                        let x = lower_hex(
                            !U256::from_dec_str(&value[1..])
                                .map_err(|_| "Can't parse into u256")?
                                + U256::from(1),
                        );
                        format!("{:f>64}", x)
                    } else {
                        U256::from_dec_str(value)
                            .map(completed_lower_hex)
                            .map_err(|_| "Can't parse into u256")?
                    };
                    StrictTokenizer::tokenize(param, &x)
                } else {
                    LenientTokenizer::tokenize(param, value)
                }
            } else {
                StrictTokenizer::tokenize(param, value)
            }
        })
        .collect::<Result<_, _>>()
        .map_err(|e| (e.to_string()))
}

/// According to the contract, encode the function and parameter values
pub fn contract_encode_input(
    contract: &Contract,
    function: &str,
    values: &[String],
    lenient: bool,
) -> Result<String, String> {
    let function = contract
        .function(function)
        .map_err(|e| (e.to_string()))?
        .clone();
    let params: Vec<_> = function
        .inputs
        .iter()
        .map(|param| param.kind.clone())
        .zip(values.iter().map(|v| v as &str))
        .collect();

    let tokens = parse_tokens(&params, lenient)?;
    let result = function
        .encode_input(&tokens)
        .map_err(|e| (e.to_string()))?;

    Ok(hex_encode(result))
}

/// According to the contract, encode the constructor and parameter values
pub fn constructor_encode_input(
    contract: &Contract,
    code: &str,
    values: &[String],
    lenient: bool,
) -> Result<String, String> {
    match contract.constructor {
        Some(ref constructor) => {
            let params: Vec<_> = constructor
                .inputs
                .iter()
                .map(|param| param.kind.clone())
                .zip(values.iter().map(|v| v as &str))
                .collect();
            let tokens = parse_tokens(&params, lenient)?;
            Ok(format!(
                "{}{}",
                code,
                hex_encode(
                    constructor
                        .encode_input(Vec::new(), &tokens)
                        .map_err(|e| (e.to_string()))?,
                )
            ))
        }
        None => Err("No constructor on abi".to_string()),
    }
}

/// According to the given abi file, encode the function and parameter values
pub fn encode_input(
    abi: &[u8],
    function: &str,
    values: &[String],
    lenient: bool,
    constructor: bool,
) -> Result<String, String> {
    let contract = Contract::load(abi).map_err(|e| format!("{}", e))?;
    if constructor {
        constructor_encode_input(&contract, function, values, lenient)
    } else {
        contract_encode_input(&contract, function, values, lenient)
    }
}

/// According to type, encode the value of the parameter
pub fn encode_params(types: &[String], values: &[String], lenient: bool) -> Result<String, String> {
    assert_eq!(types.len(), values.len());

    let types: Vec<ParamType> = types
        .iter()
        .map(|s| Reader::read(s))
        .collect::<Result<_, _>>()
        .map_err(|e| format!("{}", e))?;

    let params: Vec<_> = types
        .into_iter()
        .zip(values.iter().map(|v| v as &str))
        .collect();

    let tokens = parse_tokens(&params, lenient)?;
    let result = encode(&tokens);

    Ok(hex_encode(result))
}

/// According to type, decode the data
pub fn decode_params(types: &[String], data: &str) -> Result<Vec<String>, String> {
    let types: Vec<ParamType> = types
        .iter()
        .map(|s| Reader::read(s))
        .collect::<Result<_, _>>()
        .map_err(|e| format!("{}", e))?;

    let data = hex_decode(data).map_err(|err| err.to_string())?;

    let tokens = decode(&types, &data).map_err(|e| format!("{}", e))?;

    assert_eq!(types.len(), tokens.len());

    let result = types
        .iter()
        .zip(tokens.iter())
        .map(|(ty, to)| {
            if to.type_check(&ParamType::Bool) || format!("{}", ty) == "bool[]" {
                format!("{{\"{}\": {}}}", ty, to)
            } else {
                let to_str = format!("{}", to);
                let escaped_str = to_str.escape_default();
                format!("{{\"{}\": \"{}\"}}", ty, escaped_str)
            }
        })
        .collect::<Vec<String>>();

    Ok(result)
}

/// According to the given abi file, decode the data
pub fn decode_input(abi: &[u8], function: &str, data: &str) -> Result<Vec<String>, String> {
    let contract = Contract::load(abi).map_err(|e| format!("{}", e))?;
    let function = contract.function(function).map_err(|e| format!("{}", e))?;
    let tokens = function
        .decode_output(data.as_bytes())
        .map_err(|e| format!("{}", e))?;
    let types = function.outputs.iter().map(|ref param| &param.kind);

    assert_eq!(types.len(), tokens.len());

    let result = types
        .zip(tokens.iter())
        .map(|(ty, to)| {
            if to.type_check(&ParamType::Bool) || format!("{}", ty) == "bool[]" {
                format!("{{\"{}\": {}}}", ty, to)
            } else {
                format!("{{\"{}\": \"{}\"}}", ty, to)
            }
        })
        .collect::<Vec<String>>();

    Ok(result)
}

/// According to the given abi file, decode the topic
pub fn decode_logs(
    abi: &[u8],
    event: &str,
    topics: &[String],
    data: &str,
) -> Result<Vec<String>, String> {
    let contract = Contract::load(abi).map_err(|e| format!("{}", e))?;
    let event = contract.event(event).map_err(|e| format!("{}", e))?;

    let topics: Vec<Hash> = topics
        .iter()
        .map(|t| t.parse())
        .collect::<Result<_, _>>()
        .map_err(|e| format!("{}", e))?;
    let data = hex_decode(data).map_err(|err| err.to_string())?;
    let decoded = event
        .parse_log((topics, data).into())
        .map_err(|e| format!("{}", e))?;

    let result = decoded
        .params
        .into_iter()
        .map(|log_param| format!("{{\"{}\": \"{}\"}}", log_param.name, log_param.value))
        .collect::<Vec<String>>();

    Ok(result)
}
