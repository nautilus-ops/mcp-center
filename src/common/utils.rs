use std::env;
use regex::Regex;

pub fn replace_env_variables(input: String) -> String {
    let re = Regex::new(r#""\$\{(\w+)(?::([^}]*))?\}""#).unwrap();

    re.replace_all(&input, |caps: &regex::Captures| {
        let var_name = &caps[1];
        let default = caps.get(2).map(|m| m.as_str()).unwrap_or("");

        let val = env::var(var_name).unwrap_or_else(|_| default.to_string());

        if val.parse::<f64>().is_ok() {
            val
        } else if val == "true" || val == "false" {
            format!("{}", val)
        }else {
            format!("\"{}\"", val)
        }
    })
        .into_owned()
}
