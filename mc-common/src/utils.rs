use regex::Regex;
use std::env;

pub fn replace_env_variables(input: String) -> String {
    let re = Regex::new(r#""\$\{(\w+)(?::([^}]*))?\}""#).unwrap();

    re.replace_all(&input, |caps: &regex::Captures| {
        let var_name = &caps[1];
        let default = caps.get(2).map(|m| m.as_str()).unwrap_or("");

        let val = env::var(var_name).unwrap_or_else(|_| default.to_string());

        if val.parse::<f64>().is_ok() {
            val
        } else if val == "true" || val == "false" {
            val.to_string()
        } else {
            format!("\"{val}\"")
        }
    })
    .into_owned()
}

#[cfg(test)]
mod tests {
    use std::env;
    use crate::utils::replace_env_variables;

    #[test]
    fn test_replace_env_variables() {
        struct TestCase {
            input: &'static str,
            want: &'static str,
        }

        let tests = vec![
            TestCase {
                input: r#"self_addr = "${SELF_ADDR:http://127.0.0.1}""#,
                want: r#"self_addr = "http://127.0.0.1""#,
            },
            TestCase {
                input: r#"mcp_definition_path = "${SERVER_DEFINITION_PATH:mcp_servers.toml}""#,
                want: r#"mcp_definition_path = "mcp_servers.toml""#,
            },
            TestCase {
                input: r#"port = "${POSTGRES_PORT:5432}""#,
                want: r#"port = 5432"#,
            },
            TestCase {
                input: r#"host = "${POSTGRES_HOST}""#,
                want: r#"host = "127.0.0.1""#,
            },
        ];

        unsafe { env::set_var("POSTGRES_HOST", "127.0.0.1") }
        tests
            .into_iter()
            .for_each(|t| assert_eq!(replace_env_variables(t.input.to_string()), t.want));
    }
}
