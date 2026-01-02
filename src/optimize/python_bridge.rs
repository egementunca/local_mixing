//! Python subprocess bridge for SAT optimization.
//!
//! Communicates with the Python optimizer via JSON over stdio.

use serde::{Deserialize, Serialize};
use std::io::Write;
use std::process::{Command, Stdio};

/// Request sent to Python optimizer.
#[derive(Debug, Serialize)]
pub struct OptimizeRequest {
    pub num_inputs: usize,
    pub output_truth_tables: Vec<String>,
    pub current_num_gates: usize,
    pub time_limit: u32,
}

/// Response from Python optimizer.
#[derive(Debug, Deserialize)]
pub struct OptimizeResponse {
    pub success: bool,
    pub gates: Option<Vec<[u8; 3]>>,
    pub num_gates: Option<usize>,
    pub error: Option<String>,
}

/// Call the Python optimizer via subprocess.
///
/// # Arguments
/// * `request` - The optimization request
/// * `python_path` - Path to Python executable (default: "python3")
/// * `script_dir` - Directory containing the optimization module
///
/// # Returns
/// The optimization response, or an error message.
pub fn call_python_optimizer(
    request: &OptimizeRequest,
    python_path: Option<&str>,
    script_dir: &str,
) -> Result<OptimizeResponse, String> {
    let python = python_path.unwrap_or("python3");

    // Serialize request to JSON
    let request_json = serde_json::to_string(request)
        .map_err(|e| format!("Failed to serialize request: {}", e))?;

    // Run Python script
    let mut child = Command::new(python)
        .arg("-m")
        .arg("optimization.rust_interface")
        .current_dir(script_dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("Failed to spawn Python process: {}", e))?;

    // Write request to stdin
    {
        let stdin = child.stdin.as_mut().ok_or("Failed to open stdin")?;
        stdin
            .write_all(request_json.as_bytes())
            .map_err(|e| format!("Failed to write to stdin: {}", e))?;
    }

    // Wait for process and collect output
    let output = child
        .wait_with_output()
        .map_err(|e| format!("Failed to wait for Python process: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("Python process failed: {}", stderr));
    }

    // Parse response
    let response_str = String::from_utf8_lossy(&output.stdout);
    serde_json::from_str(&response_str)
        .map_err(|e| format!("Failed to parse response: {} (output: {})", e, response_str))
}

/// Optimize a subcircuit using the Python SAT solver.
///
/// # Arguments
/// * `truth_tables` - Truth tables for the subcircuit outputs
/// * `num_inputs` - Number of input wires
/// * `current_gates` - Current number of gates
/// * `project_root` - Root directory of the project (containing optimization/)
///
/// # Returns
/// Optimized gates if found, None otherwise.
pub fn optimize_subcircuit(
    truth_tables: Vec<String>,
    num_inputs: usize,
    current_gates: usize,
    project_root: &str,
) -> Option<Vec<[u8; 3]>> {
    let request = OptimizeRequest {
        num_inputs,
        output_truth_tables: truth_tables,
        current_num_gates: current_gates,
        time_limit: 30,
    };

    match call_python_optimizer(&request, None, project_root) {
        Ok(response) => {
            if response.success {
                response.gates
            } else {
                if let Some(err) = response.error {
                    eprintln!("Optimization error: {}", err);
                }
                None
            }
        }
        Err(e) => {
            eprintln!("Python bridge error: {}", e);
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_request_serialization() {
        let request = OptimizeRequest {
            num_inputs: 3,
            output_truth_tables: vec!["11010110".to_string()],
            current_num_gates: 5,
            time_limit: 30,
        };

        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("num_inputs"));
        assert!(json.contains("output_truth_tables"));
    }

    #[test]
    fn test_response_deserialization() {
        let json = r#"{"success": true, "gates": [[3, 0, 1]], "num_gates": 1, "error": null}"#;
        let response: OptimizeResponse = serde_json::from_str(json).unwrap();

        assert!(response.success);
        assert!(response.gates.is_some());
        assert_eq!(response.gates.unwrap().len(), 1);
    }
}
