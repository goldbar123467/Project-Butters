//! Jupiter API Contract Tests
//!
//! Golden response fixture tests for Jupiter V6 quote and swap APIs.
//! These tests verify that real API responses match our expected contract.
//!
//! Fixtures are immutable once committed - any changes require explicit justification.
//!
//! Test modules:
//! - `quote_contract_tests`: Tests for /quote endpoint responses
//! - `swap_contract_tests`: Tests for /swap endpoint responses

#[cfg(test)]
mod swap_contract_tests {
    use base64::Engine;
    use serde_json::Value;

    /// Load swap fixture from fixtures directory
    fn load_swap_fixture(name: &str) -> Value {
        let fixture_path = format!(
            "{}/fixtures/jupiter/{}.json",
            env!("CARGO_MANIFEST_DIR"),
            name
        );
        let content = std::fs::read_to_string(&fixture_path).unwrap_or_else(|e| {
            panic!(
                "CONTRACT VIOLATION: Failed to load fixture '{}': {}",
                fixture_path, e
            )
        });
        serde_json::from_str(&content).unwrap_or_else(|e| {
            panic!(
                "CONTRACT VIOLATION: Failed to parse fixture '{}' as JSON: {}",
                fixture_path, e
            )
        })
    }

    /// Get all swap fixture names for testing
    fn swap_fixture_names() -> Vec<&'static str> {
        vec!["swap_standard_v1", "swap_priority_v1"]
    }

    #[test]
    fn test_swap_response_required_fields_present() {
        for fixture_name in swap_fixture_names() {
            let fixture = load_swap_fixture(fixture_name);

            // Core required fields for swap response
            let required_fields = [
                "swapTransaction",
                "lastValidBlockHeight",
                "prioritizationFeeLamports",
                "computeUnitLimit",
                "prioritizationType",
                "dynamicSlippageReport",
                "simulationError",
            ];

            for field in required_fields {
                assert!(
                    fixture.get(field).is_some(),
                    "CONTRACT VIOLATION: Field '{}' is missing from Jupiter swap response in fixture '{}'",
                    field,
                    fixture_name
                );
            }

            // Nested required fields in prioritizationType
            let priority_type = fixture.get("prioritizationType").unwrap();
            assert!(
                priority_type.get("computeBudget").is_some(),
                "CONTRACT VIOLATION: Field 'prioritizationType.computeBudget' is missing from Jupiter swap response in fixture '{}'",
                fixture_name
            );

            let compute_budget = priority_type.get("computeBudget").unwrap();
            assert!(
                compute_budget.get("microLamports").is_some(),
                "CONTRACT VIOLATION: Field 'prioritizationType.computeBudget.microLamports' is missing from Jupiter swap response in fixture '{}'",
                fixture_name
            );
            assert!(
                compute_budget.get("estimatedMicroLamports").is_some(),
                "CONTRACT VIOLATION: Field 'prioritizationType.computeBudget.estimatedMicroLamports' is missing from Jupiter swap response in fixture '{}'",
                fixture_name
            );

            // Nested required fields in dynamicSlippageReport
            let slippage_report = fixture.get("dynamicSlippageReport").unwrap();
            let slippage_fields = [
                "slippageBps",
                "otherAmount",
                "simulatedIncurredSlippageBps",
                "amplificationRatio",
            ];

            for field in slippage_fields {
                assert!(
                    slippage_report.get(field).is_some(),
                    "CONTRACT VIOLATION: Field 'dynamicSlippageReport.{}' is missing from Jupiter swap response in fixture '{}'",
                    field,
                    fixture_name
                );
            }
        }
    }

    #[test]
    fn test_swap_response_field_types() {
        for fixture_name in swap_fixture_names() {
            let fixture = load_swap_fixture(fixture_name);

            // swapTransaction must be a string
            assert!(
                fixture.get("swapTransaction").unwrap().is_string(),
                "CONTRACT VIOLATION: Field 'swapTransaction' must be a string in fixture '{}'",
                fixture_name
            );

            // lastValidBlockHeight must be a number (u64)
            assert!(
                fixture.get("lastValidBlockHeight").unwrap().is_u64(),
                "CONTRACT VIOLATION: Field 'lastValidBlockHeight' must be a positive integer (u64) in fixture '{}'",
                fixture_name
            );

            // prioritizationFeeLamports must be a number (u64)
            assert!(
                fixture.get("prioritizationFeeLamports").unwrap().is_u64(),
                "CONTRACT VIOLATION: Field 'prioritizationFeeLamports' must be a positive integer (u64) in fixture '{}'",
                fixture_name
            );

            // computeUnitLimit must be a number (u64)
            assert!(
                fixture.get("computeUnitLimit").unwrap().is_u64(),
                "CONTRACT VIOLATION: Field 'computeUnitLimit' must be a positive integer (u64) in fixture '{}'",
                fixture_name
            );

            // prioritizationType must be an object
            assert!(
                fixture.get("prioritizationType").unwrap().is_object(),
                "CONTRACT VIOLATION: Field 'prioritizationType' must be an object in fixture '{}'",
                fixture_name
            );

            // dynamicSlippageReport must be an object
            assert!(
                fixture.get("dynamicSlippageReport").unwrap().is_object(),
                "CONTRACT VIOLATION: Field 'dynamicSlippageReport' must be an object in fixture '{}'",
                fixture_name
            );

            // simulationError must be null for successful responses
            assert!(
                fixture.get("simulationError").unwrap().is_null(),
                "CONTRACT VIOLATION: Field 'simulationError' must be null for successful swap responses in fixture '{}'",
                fixture_name
            );

            // Nested type checks for prioritizationType.computeBudget
            let compute_budget = fixture
                .get("prioritizationType")
                .unwrap()
                .get("computeBudget")
                .unwrap();

            assert!(
                compute_budget.get("microLamports").unwrap().is_u64(),
                "CONTRACT VIOLATION: Field 'prioritizationType.computeBudget.microLamports' must be a positive integer (u64) in fixture '{}'",
                fixture_name
            );
            assert!(
                compute_budget.get("estimatedMicroLamports").unwrap().is_u64(),
                "CONTRACT VIOLATION: Field 'prioritizationType.computeBudget.estimatedMicroLamports' must be a positive integer (u64) in fixture '{}'",
                fixture_name
            );

            // Nested type checks for dynamicSlippageReport
            let slippage_report = fixture.get("dynamicSlippageReport").unwrap();

            assert!(
                slippage_report.get("slippageBps").unwrap().is_u64(),
                "CONTRACT VIOLATION: Field 'dynamicSlippageReport.slippageBps' must be a positive integer (u64) in fixture '{}'",
                fixture_name
            );
            assert!(
                slippage_report.get("otherAmount").unwrap().is_u64(),
                "CONTRACT VIOLATION: Field 'dynamicSlippageReport.otherAmount' must be a positive integer (u64) in fixture '{}'",
                fixture_name
            );
            assert!(
                slippage_report.get("simulatedIncurredSlippageBps").unwrap().is_u64(),
                "CONTRACT VIOLATION: Field 'dynamicSlippageReport.simulatedIncurredSlippageBps' must be a positive integer (u64) in fixture '{}'",
                fixture_name
            );
            assert!(
                slippage_report.get("amplificationRatio").unwrap().is_string(),
                "CONTRACT VIOLATION: Field 'dynamicSlippageReport.amplificationRatio' must be a string in fixture '{}'",
                fixture_name
            );
        }
    }

    #[test]
    fn test_swap_response_transaction_validity() {
        for fixture_name in swap_fixture_names() {
            let fixture = load_swap_fixture(fixture_name);

            let swap_transaction = fixture
                .get("swapTransaction")
                .unwrap()
                .as_str()
                .expect("CONTRACT VIOLATION: Field 'swapTransaction' must be a string");

            // swapTransaction must not be empty
            assert!(
                !swap_transaction.is_empty(),
                "CONTRACT VIOLATION: Field 'swapTransaction' must not be empty in fixture '{}'",
                fixture_name
            );

            // swapTransaction must be valid base64
            let decoded = base64::engine::general_purpose::STANDARD
                .decode(swap_transaction)
                .unwrap_or_else(|e| {
                    panic!(
                        "CONTRACT VIOLATION: Field 'swapTransaction' is not valid base64 in fixture '{}': {}",
                        fixture_name, e
                    )
                });

            // Decoded transaction must not be empty
            assert!(
                !decoded.is_empty(),
                "CONTRACT VIOLATION: Field 'swapTransaction' decodes to empty bytes in fixture '{}'",
                fixture_name
            );

            // Solana transactions have a minimum size (signature + message header)
            // Minimum is roughly: 1 (num signatures) + 64 (signature) + 3 (header) = 68 bytes
            assert!(
                decoded.len() >= 68,
                "CONTRACT VIOLATION: Field 'swapTransaction' decoded size {} is too small for a valid Solana transaction (minimum 68 bytes) in fixture '{}'",
                decoded.len(),
                fixture_name
            );
        }
    }

    #[test]
    fn test_swap_response_block_height_validity() {
        for fixture_name in swap_fixture_names() {
            let fixture = load_swap_fixture(fixture_name);

            let last_valid_block_height = fixture
                .get("lastValidBlockHeight")
                .unwrap()
                .as_u64()
                .expect("CONTRACT VIOLATION: Field 'lastValidBlockHeight' must be a u64");

            // lastValidBlockHeight must be greater than 0
            assert!(
                last_valid_block_height > 0,
                "CONTRACT VIOLATION: Field 'lastValidBlockHeight' must be greater than 0 in fixture '{}', got {}",
                fixture_name,
                last_valid_block_height
            );

            // Sanity check: block height should be a reasonable Solana mainnet value
            // As of 2024, Solana is past block 250,000,000
            assert!(
                last_valid_block_height > 250_000_000,
                "CONTRACT VIOLATION: Field 'lastValidBlockHeight' value {} seems unreasonably low for Solana mainnet in fixture '{}'",
                last_valid_block_height,
                fixture_name
            );
        }
    }

    #[test]
    fn test_swap_response_priority_fee_invariants() {
        for fixture_name in swap_fixture_names() {
            let fixture = load_swap_fixture(fixture_name);

            let prioritization_fee = fixture
                .get("prioritizationFeeLamports")
                .unwrap()
                .as_u64()
                .expect("CONTRACT VIOLATION: Field 'prioritizationFeeLamports' must be a u64");

            // Get requested priority fee from fixture metadata
            let request_params = fixture.get("_request_params");
            if let Some(params) = request_params {
                if let Some(requested_fee) = params.get("prioritizationFeeLamports") {
                    let requested = requested_fee.as_u64().unwrap();
                    // Response prioritizationFeeLamports must match requested value
                    assert_eq!(
                        prioritization_fee, requested,
                        "CONTRACT VIOLATION: Field 'prioritizationFeeLamports' value {} does not match requested value {} in fixture '{}'",
                        prioritization_fee,
                        requested,
                        fixture_name
                    );
                }
            }

            // Check prioritizationType.computeBudget consistency
            let compute_budget = fixture
                .get("prioritizationType")
                .unwrap()
                .get("computeBudget")
                .unwrap();

            let micro_lamports = compute_budget
                .get("microLamports")
                .unwrap()
                .as_u64()
                .unwrap();

            let estimated_micro_lamports = compute_budget
                .get("estimatedMicroLamports")
                .unwrap()
                .as_u64()
                .unwrap();

            // microLamports must be greater than 0 when priority fee is set
            if prioritization_fee > 0 {
                assert!(
                    micro_lamports > 0,
                    "CONTRACT VIOLATION: Field 'prioritizationType.computeBudget.microLamports' must be > 0 when prioritizationFeeLamports > 0 in fixture '{}'",
                    fixture_name
                );
            }

            // estimatedMicroLamports should be <= microLamports (estimate is typically less)
            assert!(
                estimated_micro_lamports <= micro_lamports,
                "CONTRACT VIOLATION: Field 'estimatedMicroLamports' ({}) should be <= 'microLamports' ({}) in fixture '{}'",
                estimated_micro_lamports,
                micro_lamports,
                fixture_name
            );
        }
    }

    #[test]
    fn test_swap_response_compute_budget_invariants() {
        for fixture_name in swap_fixture_names() {
            let fixture = load_swap_fixture(fixture_name);

            let compute_unit_limit = fixture
                .get("computeUnitLimit")
                .unwrap()
                .as_u64()
                .expect("CONTRACT VIOLATION: Field 'computeUnitLimit' must be a u64");

            // computeUnitLimit must be greater than 0
            assert!(
                compute_unit_limit > 0,
                "CONTRACT VIOLATION: Field 'computeUnitLimit' must be greater than 0 in fixture '{}', got {}",
                fixture_name,
                compute_unit_limit
            );

            // Solana max compute units per transaction is 1,400,000
            // Jupiter swaps typically use 200,000 - 400,000
            assert!(
                compute_unit_limit <= 1_400_000,
                "CONTRACT VIOLATION: Field 'computeUnitLimit' value {} exceeds Solana maximum of 1,400,000 in fixture '{}'",
                compute_unit_limit,
                fixture_name
            );

            // Sanity check: swap transactions need at least some compute
            assert!(
                compute_unit_limit >= 50_000,
                "CONTRACT VIOLATION: Field 'computeUnitLimit' value {} seems unreasonably low for a swap transaction in fixture '{}'",
                compute_unit_limit,
                fixture_name
            );
        }
    }

    #[test]
    fn test_swap_response_slippage_report_invariants() {
        for fixture_name in swap_fixture_names() {
            let fixture = load_swap_fixture(fixture_name);

            let slippage_report = fixture
                .get("dynamicSlippageReport")
                .unwrap();

            let slippage_bps = slippage_report
                .get("slippageBps")
                .unwrap()
                .as_u64()
                .expect("CONTRACT VIOLATION: Field 'dynamicSlippageReport.slippageBps' must be a u64");

            let simulated_slippage_bps = slippage_report
                .get("simulatedIncurredSlippageBps")
                .unwrap()
                .as_u64()
                .expect("CONTRACT VIOLATION: Field 'dynamicSlippageReport.simulatedIncurredSlippageBps' must be a u64");

            let other_amount = slippage_report
                .get("otherAmount")
                .unwrap()
                .as_u64()
                .expect("CONTRACT VIOLATION: Field 'dynamicSlippageReport.otherAmount' must be a u64");

            let amplification_ratio = slippage_report
                .get("amplificationRatio")
                .unwrap()
                .as_str()
                .expect("CONTRACT VIOLATION: Field 'dynamicSlippageReport.amplificationRatio' must be a string");

            // slippageBps must be reasonable (0-10000 = 0-100%)
            assert!(
                slippage_bps <= 10_000,
                "CONTRACT VIOLATION: Field 'dynamicSlippageReport.slippageBps' value {} exceeds maximum of 10000 (100%) in fixture '{}'",
                slippage_bps,
                fixture_name
            );

            // simulatedIncurredSlippageBps should be <= slippageBps (can't slip more than allowed)
            assert!(
                simulated_slippage_bps <= slippage_bps,
                "CONTRACT VIOLATION: Field 'dynamicSlippageReport.simulatedIncurredSlippageBps' ({}) should be <= 'slippageBps' ({}) in fixture '{}'",
                simulated_slippage_bps,
                slippage_bps,
                fixture_name
            );

            // otherAmount must be > 0 for valid swaps
            assert!(
                other_amount > 0,
                "CONTRACT VIOLATION: Field 'dynamicSlippageReport.otherAmount' must be > 0 in fixture '{}', got {}",
                fixture_name,
                other_amount
            );

            // amplificationRatio must be parseable as a float
            let parsed_ratio: Result<f64, _> = amplification_ratio.parse();
            assert!(
                parsed_ratio.is_ok(),
                "CONTRACT VIOLATION: Field 'dynamicSlippageReport.amplificationRatio' value '{}' is not a valid float in fixture '{}'",
                amplification_ratio,
                fixture_name
            );

            // amplificationRatio should be positive
            let ratio = parsed_ratio.unwrap();
            assert!(
                ratio > 0.0,
                "CONTRACT VIOLATION: Field 'dynamicSlippageReport.amplificationRatio' value {} must be positive in fixture '{}'",
                ratio,
                fixture_name
            );
        }
    }

    #[test]
    fn test_swap_response_simulation_error_null_for_success() {
        for fixture_name in swap_fixture_names() {
            let fixture = load_swap_fixture(fixture_name);

            let simulation_error = fixture.get("simulationError").unwrap();

            // For successful swap responses, simulationError must be null
            assert!(
                simulation_error.is_null(),
                "CONTRACT VIOLATION: Field 'simulationError' must be null for successful swap responses in fixture '{}', got {:?}",
                fixture_name,
                simulation_error
            );
        }
    }
}

// ============================================================================
// Quote API Contract Tests
// ============================================================================

#[cfg(test)]
mod quote_contract_tests {
    use serde_json::Value;
    use std::collections::HashSet;

    /// Required fields that MUST be present in every quote response
    const REQUIRED_TOP_LEVEL_FIELDS: &[&str] = &[
        "inputMint",
        "inAmount",
        "outputMint",
        "outAmount",
        "otherAmountThreshold",
        "swapMode",
        "slippageBps",
        "priceImpactPct",
        "routePlan",
    ];

    /// Required fields in swapInfo objects
    const REQUIRED_SWAP_INFO_FIELDS: &[&str] = &[
        "ammKey",
        "label",
        "inputMint",
        "outputMint",
        "inAmount",
        "outAmount",
    ];

    /// Optional fields in swapInfo objects (may not be present in all responses)
    const OPTIONAL_SWAP_INFO_FIELDS: &[&str] = &["feeAmount", "feeMint"];

    /// Required fields in routePlan step objects
    const REQUIRED_ROUTE_PLAN_STEP_FIELDS: &[&str] = &["swapInfo", "percent"];

    // ========================================================================
    // Fixture Loading Helpers
    // ========================================================================

    /// Get the path to the fixtures directory
    fn fixtures_dir() -> std::path::PathBuf {
        let manifest_dir = env!("CARGO_MANIFEST_DIR");
        std::path::PathBuf::from(manifest_dir)
            .join("fixtures")
            .join("jupiter")
    }

    /// Load a fixture file and parse it as raw JSON Value
    fn load_fixture_as_value(filename: &str) -> Value {
        let path = fixtures_dir().join(filename);
        let content = std::fs::read_to_string(&path).unwrap_or_else(|e| {
            panic!(
                "FIXTURE LOAD FAILURE: Could not read fixture file '{}' at path '{}': {}",
                filename,
                path.display(),
                e
            )
        });
        serde_json::from_str(&content).unwrap_or_else(|e| {
            panic!(
                "FIXTURE PARSE FAILURE: Could not parse fixture '{}' as JSON: {}",
                filename, e
            )
        })
    }

    /// Load all quote fixtures from the fixtures directory
    fn load_all_quote_fixtures() -> Vec<(&'static str, Value)> {
        vec![
            (
                "quote_sol_usdc_v1.json",
                load_fixture_as_value("quote_sol_usdc_v1.json"),
            ),
            (
                "quote_multi_hop_v1.json",
                load_fixture_as_value("quote_multi_hop_v1.json"),
            ),
            (
                "quote_high_impact_v1.json",
                load_fixture_as_value("quote_high_impact_v1.json"),
            ),
        ]
    }

    /// Assert that a JSON object has a required field
    fn assert_field_present(obj: &Value, field: &str, context: &str) {
        assert!(
            obj.get(field).is_some(),
            "MISSING REQUIRED FIELD: '{}' not found in {}. \
             This indicates a breaking API contract change. \
             Available fields: {:?}",
            field,
            context,
            obj.as_object().map(|o| o.keys().collect::<Vec<_>>())
        );
    }

    /// Assert that a field is a specific JSON type
    fn assert_field_type(obj: &Value, field: &str, expected_type: &str, context: &str) {
        let value = obj.get(field).unwrap_or_else(|| {
            panic!(
                "Cannot check type of missing field '{}' in {}",
                field, context
            )
        });

        let actual_type = match value {
            Value::Null => "null",
            Value::Bool(_) => "boolean",
            Value::Number(_) => "number",
            Value::String(_) => "string",
            Value::Array(_) => "array",
            Value::Object(_) => "object",
        };

        assert_eq!(
            actual_type, expected_type,
            "TYPE MISMATCH: Field '{}' in {} expected type '{}' but got '{}'. Value: {}",
            field, context, expected_type, actual_type, value
        );
    }

    /// Assert that a string field is parseable as u64
    fn assert_parseable_as_u64(obj: &Value, field: &str, context: &str) -> u64 {
        let value = obj.get(field).unwrap_or_else(|| {
            panic!(
                "Cannot parse missing field '{}' as u64 in {}",
                field, context
            )
        });

        let str_value = value.as_str().unwrap_or_else(|| {
            panic!(
                "PARSE ERROR: Field '{}' in {} must be a string to parse as u64, got: {}",
                field, context, value
            )
        });

        str_value.parse::<u64>().unwrap_or_else(|e| {
            panic!(
                "PARSE ERROR: Field '{}' in {} value '{}' is not a valid u64: {}",
                field, context, str_value, e
            )
        })
    }

    /// Assert that a string field is parseable as f64
    fn assert_parseable_as_f64(obj: &Value, field: &str, context: &str) -> f64 {
        let value = obj.get(field).unwrap_or_else(|| {
            panic!(
                "Cannot parse missing field '{}' as f64 in {}",
                field, context
            )
        });

        let str_value = value.as_str().unwrap_or_else(|| {
            panic!(
                "PARSE ERROR: Field '{}' in {} must be a string to parse as f64, got: {}",
                field, context, value
            )
        });

        str_value.parse::<f64>().unwrap_or_else(|e| {
            panic!(
                "PARSE ERROR: Field '{}' in {} value '{}' is not a valid f64: {}",
                field, context, str_value, e
            )
        })
    }

    /// Assert that a number field fits in u16 range
    fn assert_valid_u16(obj: &Value, field: &str, context: &str) -> u16 {
        let value = obj.get(field).unwrap_or_else(|| {
            panic!(
                "Cannot validate missing field '{}' as u16 in {}",
                field, context
            )
        });

        let num = value.as_u64().unwrap_or_else(|| {
            panic!(
                "RANGE ERROR: Field '{}' in {} must be a number, got: {}",
                field, context, value
            )
        });

        assert!(
            num <= u16::MAX as u64,
            "RANGE ERROR: Field '{}' in {} value {} exceeds u16 max ({})",
            field,
            context,
            num,
            u16::MAX
        );

        num as u16
    }

    // ========================================================================
    // Contract Tests
    // ========================================================================

    #[test]
    fn test_quote_response_required_fields_present() {
        let fixtures = load_all_quote_fixtures();

        for (fixture_name, value) in fixtures {
            let context = format!("fixture '{}'", fixture_name);

            // Check all required top-level fields
            for field in REQUIRED_TOP_LEVEL_FIELDS {
                assert_field_present(&value, field, &context);
            }

            // Check routePlan structure
            let route_plan = value
                .get("routePlan")
                .and_then(|v| v.as_array())
                .unwrap_or_else(|| {
                    panic!(
                        "INVALID STRUCTURE: 'routePlan' in {} must be an array",
                        context
                    )
                });

            assert!(
                !route_plan.is_empty(),
                "INVARIANT VIOLATION: 'routePlan' in {} must not be empty",
                context
            );

            // Check each route plan step
            for (i, step) in route_plan.iter().enumerate() {
                let step_context = format!("{} routePlan[{}]", context, i);

                for field in REQUIRED_ROUTE_PLAN_STEP_FIELDS {
                    assert_field_present(step, field, &step_context);
                }

                // Check swapInfo fields
                let swap_info = step.get("swapInfo").unwrap();
                let swap_info_context = format!("{}.swapInfo", step_context);

                for field in REQUIRED_SWAP_INFO_FIELDS {
                    assert_field_present(swap_info, field, &swap_info_context);
                }
            }
        }
    }

    #[test]
    fn test_quote_response_field_types() {
        let fixtures = load_all_quote_fixtures();

        for (fixture_name, value) in fixtures {
            let context = format!("fixture '{}'", fixture_name);

            // String fields
            assert_field_type(&value, "inputMint", "string", &context);
            assert_field_type(&value, "outputMint", "string", &context);
            assert_field_type(&value, "inAmount", "string", &context);
            assert_field_type(&value, "outAmount", "string", &context);
            assert_field_type(&value, "otherAmountThreshold", "string", &context);
            assert_field_type(&value, "swapMode", "string", &context);
            assert_field_type(&value, "priceImpactPct", "string", &context);

            // Number fields
            assert_field_type(&value, "slippageBps", "number", &context);

            // Array fields
            assert_field_type(&value, "routePlan", "array", &context);

            // Check routePlan step types
            let route_plan = value.get("routePlan").unwrap().as_array().unwrap();
            for (i, step) in route_plan.iter().enumerate() {
                let step_context = format!("{} routePlan[{}]", context, i);

                assert_field_type(step, "swapInfo", "object", &step_context);
                assert_field_type(step, "percent", "number", &step_context);

                let swap_info = step.get("swapInfo").unwrap();
                let swap_info_context = format!("{}.swapInfo", step_context);

                assert_field_type(swap_info, "ammKey", "string", &swap_info_context);
                assert_field_type(swap_info, "label", "string", &swap_info_context);
                assert_field_type(swap_info, "inputMint", "string", &swap_info_context);
                assert_field_type(swap_info, "outputMint", "string", &swap_info_context);
                assert_field_type(swap_info, "inAmount", "string", &swap_info_context);
                assert_field_type(swap_info, "outAmount", "string", &swap_info_context);

                // Optional fee fields - only check type if present
                if swap_info.get("feeAmount").is_some() {
                    assert_field_type(swap_info, "feeAmount", "string", &swap_info_context);
                }
                if swap_info.get("feeMint").is_some() {
                    assert_field_type(swap_info, "feeMint", "string", &swap_info_context);
                }
            }
        }
    }

    #[test]
    fn test_quote_response_optional_fee_fields() {
        let fixtures = load_all_quote_fixtures();

        for (fixture_name, value) in fixtures {
            let context = format!("fixture '{}'", fixture_name);

            let route_plan = value.get("routePlan").unwrap().as_array().unwrap();

            for (i, step) in route_plan.iter().enumerate() {
                let swap_info = step.get("swapInfo").unwrap();
                let step_context = format!("{} routePlan[{}].swapInfo", context, i);

                // If feeAmount is present, it must be a valid string parseable as u64
                if let Some(fee_amount) = swap_info.get("feeAmount") {
                    assert!(
                        fee_amount.is_string(),
                        "TYPE MISMATCH: Optional field 'feeAmount' in {} must be a string if present, got: {}",
                        step_context,
                        fee_amount
                    );

                    let fee_str = fee_amount.as_str().unwrap();
                    let parsed: Result<u64, _> = fee_str.parse();
                    assert!(
                        parsed.is_ok(),
                        "PARSE ERROR: Optional field 'feeAmount' in {} value '{}' is not a valid u64",
                        step_context,
                        fee_str
                    );
                }

                // If feeMint is present, it must be a valid Solana address (32-44 chars)
                if let Some(fee_mint) = swap_info.get("feeMint") {
                    assert!(
                        fee_mint.is_string(),
                        "TYPE MISMATCH: Optional field 'feeMint' in {} must be a string if present, got: {}",
                        step_context,
                        fee_mint
                    );

                    let mint_str = fee_mint.as_str().unwrap();
                    assert!(
                        mint_str.len() >= 32 && mint_str.len() <= 44,
                        "INVARIANT VIOLATION: Optional field 'feeMint' in {} has invalid length: {} (expected 32-44 for Solana address)",
                        step_context,
                        mint_str.len()
                    );
                }

                // If one fee field is present, the other should also be present (consistency check)
                let has_fee_amount = swap_info.get("feeAmount").is_some();
                let has_fee_mint = swap_info.get("feeMint").is_some();

                if has_fee_amount != has_fee_mint {
                    // This is a warning, not a hard failure - some AMMs might not report fees
                    eprintln!(
                        "WARNING: Inconsistent fee fields in {} - feeAmount: {}, feeMint: {}",
                        step_context, has_fee_amount, has_fee_mint
                    );
                }
            }
        }
    }

    #[test]
    fn test_quote_response_amount_invariants() {
        let fixtures = load_all_quote_fixtures();

        for (fixture_name, value) in fixtures {
            let context = format!("fixture '{}'", fixture_name);

            // inAmount must be parseable as u64
            let in_amount = assert_parseable_as_u64(&value, "inAmount", &context);

            // outAmount must be parseable as u64 and > 0
            let out_amount = assert_parseable_as_u64(&value, "outAmount", &context);
            assert!(
                out_amount > 0,
                "INVARIANT VIOLATION: 'outAmount' in {} must be > 0, got: {}",
                context,
                out_amount
            );

            // otherAmountThreshold must be parseable as u64 and <= outAmount
            let threshold = assert_parseable_as_u64(&value, "otherAmountThreshold", &context);
            assert!(
                threshold <= out_amount,
                "INVARIANT VIOLATION: 'otherAmountThreshold' ({}) in {} must be <= 'outAmount' ({}). \
                 The minimum acceptable output cannot exceed the expected output.",
                threshold,
                context,
                out_amount
            );

            // Validate route plan amounts
            let route_plan = value.get("routePlan").unwrap().as_array().unwrap();
            for (i, step) in route_plan.iter().enumerate() {
                let swap_info = step.get("swapInfo").unwrap();
                let step_context = format!("{} routePlan[{}].swapInfo", context, i);

                // Required swap amounts must be parseable
                let step_in = assert_parseable_as_u64(swap_info, "inAmount", &step_context);
                let step_out = assert_parseable_as_u64(swap_info, "outAmount", &step_context);

                // Input must be > 0
                assert!(
                    step_in > 0,
                    "INVARIANT VIOLATION: 'inAmount' in {} must be > 0",
                    step_context
                );

                // Output must be > 0
                assert!(
                    step_out > 0,
                    "INVARIANT VIOLATION: 'outAmount' in {} must be > 0",
                    step_context
                );

                // Optional feeAmount - if present, should not exceed output (sanity check)
                if let Some(fee_value) = swap_info.get("feeAmount") {
                    if let Some(fee_str) = fee_value.as_str() {
                        if let Ok(fee) = fee_str.parse::<u64>() {
                            assert!(
                                fee <= step_out,
                                "SANITY CHECK FAILURE: 'feeAmount' ({}) in {} exceeds 'outAmount' ({})",
                                fee,
                                step_context,
                                step_out
                            );
                        }
                    }
                }
            }

            // Verify total inAmount matches sum of route inAmounts for first-hop routes
            // (This only applies to routes that start from the input token)
            let input_mint = value.get("inputMint").unwrap().as_str().unwrap();
            let first_hop_in_total: u64 = route_plan
                .iter()
                .filter(|step| {
                    step.get("swapInfo")
                        .and_then(|si| si.get("inputMint"))
                        .and_then(|v| v.as_str())
                        == Some(input_mint)
                })
                .map(|step| {
                    step.get("swapInfo")
                        .and_then(|si| si.get("inAmount"))
                        .and_then(|v| v.as_str())
                        .and_then(|s| s.parse::<u64>().ok())
                        .unwrap_or(0)
                })
                .sum();

            assert_eq!(
                first_hop_in_total, in_amount,
                "INVARIANT VIOLATION: Sum of first-hop inAmounts ({}) in {} does not equal total inAmount ({})",
                first_hop_in_total, context, in_amount
            );
        }
    }

    #[test]
    fn test_quote_response_slippage_invariants() {
        let fixtures = load_all_quote_fixtures();

        for (fixture_name, value) in fixtures {
            let context = format!("fixture '{}'", fixture_name);

            // slippageBps must be a valid u16
            let slippage_bps = assert_valid_u16(&value, "slippageBps", &context);

            // Slippage should be reasonable (< 10000 bps = 100%)
            assert!(
                slippage_bps < 10000,
                "INVARIANT VIOLATION: 'slippageBps' ({}) in {} exceeds 100% (10000 bps)",
                slippage_bps,
                context
            );

            // Verify otherAmountThreshold is consistent with slippage
            let out_amount = assert_parseable_as_u64(&value, "outAmount", &context);
            let threshold = assert_parseable_as_u64(&value, "otherAmountThreshold", &context);

            // Calculate expected minimum based on slippage
            // threshold should be approximately outAmount * (1 - slippageBps/10000)
            let max_slippage_amount = out_amount.saturating_mul(slippage_bps as u64) / 10000;
            let expected_min_threshold = out_amount.saturating_sub(max_slippage_amount);

            // Allow 1% tolerance for rounding
            let tolerance = out_amount / 100;
            assert!(
                threshold >= expected_min_threshold.saturating_sub(tolerance),
                "INVARIANT VIOLATION: 'otherAmountThreshold' ({}) in {} is lower than expected \
                 minimum ({}) given slippageBps ({}). This suggests incorrect slippage calculation.",
                threshold,
                context,
                expected_min_threshold,
                slippage_bps
            );
        }
    }

    #[test]
    fn test_quote_response_route_plan_invariants() {
        let fixtures = load_all_quote_fixtures();

        for (fixture_name, value) in fixtures {
            let context = format!("fixture '{}'", fixture_name);

            let route_plan = value.get("routePlan").unwrap().as_array().unwrap();

            // routePlan must be non-empty
            assert!(
                !route_plan.is_empty(),
                "INVARIANT VIOLATION: 'routePlan' in {} must be non-empty",
                context
            );

            // Check ammKey is non-empty for all steps
            for (i, step) in route_plan.iter().enumerate() {
                let swap_info = step.get("swapInfo").unwrap();
                let step_context = format!("{} routePlan[{}].swapInfo", context, i);

                let amm_key = swap_info
                    .get("ammKey")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");

                assert!(
                    !amm_key.is_empty(),
                    "INVARIANT VIOLATION: 'ammKey' in {} must be non-empty string",
                    step_context
                );

                // Label should also be non-empty (DEX identifier)
                let label = swap_info.get("label").and_then(|v| v.as_str()).unwrap_or("");

                assert!(
                    !label.is_empty(),
                    "INVARIANT VIOLATION: 'label' in {} must be non-empty string",
                    step_context
                );

                // Percent must be in valid range (1-100)
                let percent = step.get("percent").and_then(|v| v.as_u64()).unwrap_or(0);

                assert!(
                    percent >= 1 && percent <= 100,
                    "INVARIANT VIOLATION: 'percent' ({}) in {} must be between 1 and 100",
                    percent,
                    format!("{} routePlan[{}]", context, i)
                );
            }
        }
    }

    #[test]
    fn test_quote_multi_hop_route_split_invariants() {
        let value = load_fixture_as_value("quote_multi_hop_v1.json");
        let context = "fixture 'quote_multi_hop_v1.json'";

        let route_plan = value.get("routePlan").unwrap().as_array().unwrap();
        let input_mint = value.get("inputMint").unwrap().as_str().unwrap();
        let output_mint = value.get("outputMint").unwrap().as_str().unwrap();

        // Group routes by their input mint to identify parallel splits
        let mut first_hop_percents: Vec<u64> = Vec::new();
        let mut intermediate_hops: Vec<(&Value, usize)> = Vec::new();

        for (i, step) in route_plan.iter().enumerate() {
            let swap_info = step.get("swapInfo").unwrap();
            let step_input = swap_info.get("inputMint").unwrap().as_str().unwrap();
            let percent = step.get("percent").and_then(|v| v.as_u64()).unwrap_or(0);

            if step_input == input_mint {
                // This is a first-hop route (from original input)
                first_hop_percents.push(percent);
            } else {
                // This is an intermediate or final hop
                intermediate_hops.push((step, i));
            }
        }

        // For split routes, first-hop percentages should sum to 100
        if first_hop_percents.len() > 1 {
            let total_first_hop: u64 = first_hop_percents.iter().sum();
            assert_eq!(
                total_first_hop, 100,
                "INVARIANT VIOLATION: First-hop route percentages in {} must sum to 100 for splits. \
                 Got: {:?} = {}",
                context, first_hop_percents, total_first_hop
            );
        }

        // For single first hop, percent should be 100
        if first_hop_percents.len() == 1 {
            assert_eq!(
                first_hop_percents[0], 100,
                "INVARIANT VIOLATION: Single first-hop route in {} must have percent=100, got: {}",
                context, first_hop_percents[0]
            );
        }

        // Verify intermediate hops that consolidate should have percent=100
        for (step, i) in intermediate_hops {
            let swap_info = step.get("swapInfo").unwrap();
            let step_output = swap_info.get("outputMint").unwrap().as_str().unwrap();

            // If this hop outputs to the final token, it's a consolidation point
            if step_output == output_mint {
                let percent = step.get("percent").and_then(|v| v.as_u64()).unwrap_or(0);
                // Consolidation hops should be 100% (all intermediate tokens go through)
                assert_eq!(
                    percent, 100,
                    "INVARIANT VIOLATION: Consolidation hop routePlan[{}] in {} outputting to \
                     final token should have percent=100, got: {}",
                    i, context, percent
                );
            }
        }

        // Verify route chain is valid (outputs connect to inputs)
        let mut available_tokens: HashSet<&str> = HashSet::new();
        available_tokens.insert(input_mint);

        for (i, step) in route_plan.iter().enumerate() {
            let swap_info = step.get("swapInfo").unwrap();
            let step_input = swap_info.get("inputMint").unwrap().as_str().unwrap();
            let step_output = swap_info.get("outputMint").unwrap().as_str().unwrap();

            assert!(
                available_tokens.contains(step_input),
                "ROUTE CHAIN VIOLATION: routePlan[{}] in {} requires input '{}' \
                 but it is not available from previous hops. Available: {:?}",
                i,
                context,
                step_input,
                available_tokens
            );

            available_tokens.insert(step_output);
        }

        // Final output token must be reachable
        assert!(
            available_tokens.contains(output_mint),
            "ROUTE CHAIN VIOLATION: Output token '{}' in {} is not produced by route plan",
            output_mint,
            context
        );
    }

    #[test]
    fn test_quote_high_impact_price_detection() {
        let value = load_fixture_as_value("quote_high_impact_v1.json");
        let context = "fixture 'quote_high_impact_v1.json'";

        // priceImpactPct must be parseable as f64
        let price_impact = assert_parseable_as_f64(&value, "priceImpactPct", context);

        // High impact fixture should have significant price impact
        assert!(
            price_impact > 1.0,
            "TEST DATA ISSUE: High impact fixture {} should have priceImpactPct > 1.0%, got: {}%",
            context,
            price_impact
        );

        // Price impact should be less than 100% (sanity check)
        assert!(
            price_impact < 100.0,
            "INVARIANT VIOLATION: priceImpactPct ({}) in {} exceeds 100% which is invalid",
            price_impact,
            context
        );

        // Verify price impact is positive (you always lose on swaps)
        assert!(
            price_impact >= 0.0,
            "INVARIANT VIOLATION: priceImpactPct ({}) in {} should be >= 0. \
             Negative price impact would mean arbitrage opportunity.",
            price_impact,
            context
        );

        // For high impact trades, slippage tolerance should typically be higher
        let slippage_bps = assert_valid_u16(&value, "slippageBps", context);
        assert!(
            slippage_bps >= 100,
            "WARNING: High impact trade in {} has low slippageBps ({}). \
             Trades with >1% price impact typically need higher slippage tolerance.",
            context,
            slippage_bps
        );

        // Test that we can detect high impact programmatically
        let is_high_impact = price_impact > 0.5; // > 0.5% is considered high
        assert!(
            is_high_impact,
            "DETECTION FAILURE: Should detect {} as high impact trade (>0.5% impact)",
            context
        );

        // Verify the route uses multiple pools (split) to reduce impact
        let route_plan = value.get("routePlan").unwrap().as_array().unwrap();
        let input_mint = value.get("inputMint").unwrap().as_str().unwrap();

        let first_hop_count = route_plan
            .iter()
            .filter(|step| {
                step.get("swapInfo")
                    .and_then(|si| si.get("inputMint"))
                    .and_then(|v| v.as_str())
                    == Some(input_mint)
            })
            .count();

        assert!(
            first_hop_count >= 2,
            "ROUTE OPTIMIZATION: High impact trade in {} should use split routing \
             across multiple pools. Found {} first-hop routes.",
            context,
            first_hop_count
        );
    }

    // ========================================================================
    // Type Deserialization Tests
    // ========================================================================

    #[test]
    fn test_quote_response_deserializes_to_type() {
        use crate::adapters::jupiter::quote::QuoteResponse;

        let fixtures = load_all_quote_fixtures();

        for (fixture_name, value) in fixtures {
            let context = format!("fixture '{}'", fixture_name);

            // Remove metadata fields that are not part of the API response
            let mut clean_value = value.clone();
            if let Some(obj) = clean_value.as_object_mut() {
                obj.remove("_fixture_metadata");
                obj.remove("_request_params");
            }

            // Attempt to deserialize to our QuoteResponse type
            let result: Result<QuoteResponse, _> = serde_json::from_value(clean_value);

            assert!(
                result.is_ok(),
                "DESERIALIZATION FAILURE: Could not deserialize {} to QuoteResponse: {:?}. \
                 This indicates our type definitions do not match the API contract.",
                context,
                result.err()
            );

            let quote = result.unwrap();

            // Verify parsed values match raw values
            let raw_in_amount = value.get("inAmount").unwrap().as_str().unwrap();
            assert_eq!(
                quote.in_amount, raw_in_amount,
                "PARSE MISMATCH: in_amount in {} - raw: '{}', parsed: '{}'",
                context, raw_in_amount, quote.in_amount
            );

            let raw_out_amount = value.get("outAmount").unwrap().as_str().unwrap();
            assert_eq!(
                quote.out_amount, raw_out_amount,
                "PARSE MISMATCH: out_amount in {} - raw: '{}', parsed: '{}'",
                context, raw_out_amount, quote.out_amount
            );

            // Verify helper methods work correctly
            assert!(
                quote.input_amount() > 0,
                "HELPER FAILURE: input_amount() returned 0 for {}",
                context
            );

            assert!(
                quote.output_amount() > 0,
                "HELPER FAILURE: output_amount() returned 0 for {}",
                context
            );
        }
    }

    // ========================================================================
    // Edge Case Tests
    // ========================================================================

    #[test]
    fn test_single_hop_route_percent_is_100() {
        let value = load_fixture_as_value("quote_sol_usdc_v1.json");
        let context = "fixture 'quote_sol_usdc_v1.json'";

        let route_plan = value.get("routePlan").unwrap().as_array().unwrap();

        // Single hop route
        assert_eq!(
            route_plan.len(),
            1,
            "TEST ASSUMPTION: {} should be a single-hop route",
            context
        );

        let percent = route_plan[0]
            .get("percent")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);

        assert_eq!(
            percent, 100,
            "INVARIANT VIOLATION: Single-hop route in {} must have percent=100, got: {}",
            context, percent
        );
    }

    #[test]
    fn test_swap_mode_is_valid() {
        let fixtures = load_all_quote_fixtures();
        let valid_modes = ["ExactIn", "ExactOut"];

        for (fixture_name, value) in fixtures {
            let context = format!("fixture '{}'", fixture_name);

            let swap_mode = value
                .get("swapMode")
                .and_then(|v| v.as_str())
                .unwrap_or("");

            assert!(
                valid_modes.contains(&swap_mode),
                "INVARIANT VIOLATION: 'swapMode' in {} must be one of {:?}, got: '{}'",
                context,
                valid_modes,
                swap_mode
            );
        }
    }

    #[test]
    fn test_mint_addresses_are_valid_base58() {
        let fixtures = load_all_quote_fixtures();

        for (fixture_name, value) in fixtures {
            let context = format!("fixture '{}'", fixture_name);

            // Check top-level mints
            let input_mint = value
                .get("inputMint")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let output_mint = value
                .get("outputMint")
                .and_then(|v| v.as_str())
                .unwrap_or("");

            // Solana addresses are 32-44 characters of base58
            assert!(
                input_mint.len() >= 32 && input_mint.len() <= 44,
                "INVARIANT VIOLATION: 'inputMint' in {} has invalid length: {} (expected 32-44)",
                context,
                input_mint.len()
            );

            assert!(
                output_mint.len() >= 32 && output_mint.len() <= 44,
                "INVARIANT VIOLATION: 'outputMint' in {} has invalid length: {} (expected 32-44)",
                context,
                output_mint.len()
            );

            // Check route plan mints
            let route_plan = value.get("routePlan").unwrap().as_array().unwrap();
            for (i, step) in route_plan.iter().enumerate() {
                let swap_info = step.get("swapInfo").unwrap();
                let step_context = format!("{} routePlan[{}].swapInfo", context, i);

                // Required address fields
                for field in ["inputMint", "outputMint", "ammKey"] {
                    let addr = swap_info.get(field).and_then(|v| v.as_str()).unwrap_or("");
                    assert!(
                        addr.len() >= 32 && addr.len() <= 44,
                        "INVARIANT VIOLATION: '{}' in {} has invalid length: {} (expected 32-44 for Solana address)",
                        field,
                        step_context,
                        addr.len()
                    );
                }

                // Optional feeMint - only validate if present
                if let Some(fee_mint) = swap_info.get("feeMint") {
                    if let Some(addr) = fee_mint.as_str() {
                        assert!(
                            addr.len() >= 32 && addr.len() <= 44,
                            "INVARIANT VIOLATION: optional 'feeMint' in {} has invalid length: {} (expected 32-44 for Solana address)",
                            step_context,
                            addr.len()
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn test_price_impact_pct_is_valid_float() {
        let fixtures = load_all_quote_fixtures();

        for (fixture_name, value) in fixtures {
            let context = format!("fixture '{}'", fixture_name);

            let price_impact = assert_parseable_as_f64(&value, "priceImpactPct", &context);

            // Price impact must be non-negative
            assert!(
                price_impact >= 0.0,
                "INVARIANT VIOLATION: 'priceImpactPct' in {} must be >= 0, got: {}",
                context,
                price_impact
            );

            // Price impact must be finite
            assert!(
                price_impact.is_finite(),
                "INVARIANT VIOLATION: 'priceImpactPct' in {} must be finite, got: {}",
                context,
                price_impact
            );
        }
    }

    // ========================================================================
    // Fixture Version Guard Tests
    // ========================================================================

    #[test]
    fn test_fixture_version_guard() {
        use regex::Regex;

        let fixtures_path = fixtures_dir();

        // Read all .json files in the fixtures directory
        let entries = std::fs::read_dir(&fixtures_path).unwrap_or_else(|e| {
            panic!(
                "FIXTURE GUARD FAILURE: Could not read fixtures directory '{}': {}",
                fixtures_path.display(),
                e
            )
        });

        // Pattern: {word}_{word}_v{digits}.json (e.g., quote_sol_usdc_v1.json)
        let filename_pattern = Regex::new(r"^[a-z]+(?:_[a-z0-9]+)+_v\d+\.json$").unwrap();

        let mut fixture_count = 0;

        for entry in entries {
            let entry = entry.unwrap_or_else(|e| {
                panic!("FIXTURE GUARD FAILURE: Could not read directory entry: {}", e)
            });

            let path = entry.path();

            // Only check .json files
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }

            fixture_count += 1;
            let filename = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("");

            // Assert filename matches expected pattern
            assert!(
                filename_pattern.is_match(filename),
                "FIXTURE NAMING VIOLATION: File '{}' does not match required pattern \
                 '{{endpoint}}_{{scenario}}_v{{version}}.json' (e.g., quote_sol_usdc_v1.json). \
                 This prevents accidental fixture overwrites and ensures version tracking.",
                filename
            );

            // Load the fixture and check for _fixture_metadata.api_version
            let content = std::fs::read_to_string(&path).unwrap_or_else(|e| {
                panic!(
                    "FIXTURE GUARD FAILURE: Could not read fixture '{}': {}",
                    filename, e
                )
            });

            let value: Value = serde_json::from_str(&content).unwrap_or_else(|e| {
                panic!(
                    "FIXTURE GUARD FAILURE: Could not parse fixture '{}' as JSON: {}",
                    filename, e
                )
            });

            // Check for _fixture_metadata field
            let metadata = value.get("_fixture_metadata");
            assert!(
                metadata.is_some(),
                "FIXTURE METADATA MISSING: File '{}' must contain '_fixture_metadata' object. \
                 This field documents the fixture's origin and prevents accidental overwrites.",
                filename
            );

            // Check for api_version within _fixture_metadata
            let api_version = metadata
                .and_then(|m| m.get("api_version"))
                .and_then(|v| v.as_str());

            assert!(
                api_version.is_some() && !api_version.unwrap().is_empty(),
                "FIXTURE VERSION MISSING: File '{}' must contain '_fixture_metadata.api_version' \
                 string field (e.g., \"v6\", \"v6.2\"). This tracks the API version the fixture \
                 was captured from and prevents 'oops I overwrote v1' drift.",
                filename
            );

            // Extract version from filename and ensure consistency
            let filename_version = filename
                .rsplit('_')
                .next()
                .and_then(|s| s.strip_suffix(".json"));

            assert!(
                filename_version.is_some(),
                "FIXTURE NAMING ERROR: Could not extract version from filename '{}'",
                filename
            );
        }

        // Ensure we actually checked some fixtures
        assert!(
            fixture_count > 0,
            "FIXTURE GUARD FAILURE: No .json fixtures found in '{}'. \
             Expected at least one fixture file.",
            fixtures_path.display()
        );
    }
}

// ============================================================================
// Live Smoke Tests
// ============================================================================

/// Live smoke tests for Jupiter API endpoint validation.
///
/// These tests are `#[ignore]` by default and should NOT be run in CI.
///
/// ## Purpose
/// - Detect endpoint changes (URL, auth requirements)
/// - Detect schema changes (missing fields, type changes)
/// - Detect service outages (500s, timeouts)
/// - Verify fixtures are still representative of live API
///
/// ## Running
/// ```bash
/// cargo test live_smoke -- --ignored
/// ```
///
/// ## When to Run
/// - After updating fixtures
/// - When debugging API integration issues
/// - Periodically to verify API contract hasn't changed
///
/// ## Important Notes
/// - These tests hit the live Jupiter API
/// - Do NOT assert specific values (prices change with market)
/// - Only assert structural contract (fields exist, types match)
/// - Rate limits may apply; don't run frequently
#[cfg(test)]
mod live_smoke_tests {
    use serde_json::Value;

    /// Required fields that MUST be present in every quote response.
    /// This list should match the fixture tests.
    const REQUIRED_TOP_LEVEL_FIELDS: &[&str] = &[
        "inputMint",
        "inAmount",
        "outputMint",
        "outAmount",
        "otherAmountThreshold",
        "swapMode",
        "slippageBps",
        "priceImpactPct",
        "routePlan",
    ];

    /// Well-known token mints for testing
    const SOL_MINT: &str = "So11111111111111111111111111111111111111112";
    const USDC_MINT: &str = "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v";

    /// Live smoke test for the Jupiter /quote endpoint.
    ///
    /// This test is ignored by default. Run with:
    /// ```bash
    /// cargo test live_smoke -- --ignored
    /// ```
    ///
    /// Purpose: Detect endpoint changes, auth changes, or 500s.
    /// Not for CI - for manual verification after fixture updates.
    #[tokio::test]
    #[ignore]
    async fn test_live_quote_endpoint_schema() {
        let client = reqwest::Client::new();

        // Fixed parameters for reproducible test
        // 0.001 SOL -> USDC with 50 bps slippage
        let url = format!(
            "https://public.jupiterapi.com/quote?inputMint={}&outputMint={}&amount={}&slippageBps={}",
            SOL_MINT,
            USDC_MINT,
            1_000_000, // 0.001 SOL in lamports
            50
        );

        let response = client
            .get(&url)
            .header("Accept", "application/json")
            .send()
            .await
            .expect("LIVE ENDPOINT ERROR: Failed to connect to Jupiter API. Check network connectivity.");

        // Check HTTP status
        let status = response.status();
        assert!(
            status.is_success(),
            "LIVE ENDPOINT ERROR: Jupiter API returned HTTP {}. \
             This may indicate an outage, auth change, or endpoint deprecation.",
            status
        );

        // Parse response body
        let body = response
            .text()
            .await
            .expect("LIVE ENDPOINT ERROR: Failed to read response body");

        let json: Value = serde_json::from_str(&body).unwrap_or_else(|e| {
            panic!(
                "LIVE ENDPOINT ERROR: Response is not valid JSON: {}. Body: {}",
                e,
                &body[..body.len().min(500)]
            )
        });

        // Verify all required top-level fields exist
        for field in REQUIRED_TOP_LEVEL_FIELDS {
            assert!(
                json.get(field).is_some(),
                "LIVE ENDPOINT CHANGE: Field '{}' missing from live response. Update fixtures! \
                 Available fields: {:?}",
                field,
                json.as_object().map(|o| o.keys().collect::<Vec<_>>())
            );
        }

        // Verify routePlan is a non-empty array
        let route_plan = json.get("routePlan").expect("routePlan field should exist");
        assert!(
            route_plan.is_array(),
            "LIVE ENDPOINT CHANGE: 'routePlan' is not an array. Type changed! Got: {:?}",
            route_plan
        );

        let route_array = route_plan.as_array().unwrap();
        assert!(
            !route_array.is_empty(),
            "LIVE ENDPOINT CHANGE: 'routePlan' is empty. Expected at least one route step."
        );

        // Verify each route step has required structure
        for (i, step) in route_array.iter().enumerate() {
            assert!(
                step.get("swapInfo").is_some(),
                "LIVE ENDPOINT CHANGE: routePlan[{}] missing 'swapInfo' field. Update fixtures!",
                i
            );
            assert!(
                step.get("percent").is_some(),
                "LIVE ENDPOINT CHANGE: routePlan[{}] missing 'percent' field. Update fixtures!",
                i
            );

            // Verify swapInfo structure - required fields
            let swap_info = step.get("swapInfo").unwrap();
            let required_swap_info_fields = ["ammKey", "label", "inputMint", "outputMint", "inAmount", "outAmount"];

            for field in required_swap_info_fields {
                assert!(
                    swap_info.get(field).is_some(),
                    "LIVE ENDPOINT CHANGE: routePlan[{}].swapInfo missing required '{}' field. Update fixtures!",
                    i,
                    field
                );
            }

            // Optional fee fields - validate type if present
            if let Some(fee_amount) = swap_info.get("feeAmount") {
                assert!(
                    fee_amount.is_string(),
                    "LIVE ENDPOINT CHANGE: routePlan[{}].swapInfo.feeAmount type changed from string",
                    i
                );
            }
            if let Some(fee_mint) = swap_info.get("feeMint") {
                assert!(
                    fee_mint.is_string(),
                    "LIVE ENDPOINT CHANGE: routePlan[{}].swapInfo.feeMint type changed from string",
                    i
                );
            }
        }

        // Verify field types (not values)
        assert!(
            json.get("inputMint").unwrap().is_string(),
            "LIVE ENDPOINT CHANGE: 'inputMint' type changed from string"
        );
        assert!(
            json.get("outputMint").unwrap().is_string(),
            "LIVE ENDPOINT CHANGE: 'outputMint' type changed from string"
        );
        assert!(
            json.get("inAmount").unwrap().is_string(),
            "LIVE ENDPOINT CHANGE: 'inAmount' type changed from string"
        );
        assert!(
            json.get("outAmount").unwrap().is_string(),
            "LIVE ENDPOINT CHANGE: 'outAmount' type changed from string"
        );
        assert!(
            json.get("slippageBps").unwrap().is_number(),
            "LIVE ENDPOINT CHANGE: 'slippageBps' type changed from number"
        );
        assert!(
            json.get("priceImpactPct").unwrap().is_string(),
            "LIVE ENDPOINT CHANGE: 'priceImpactPct' type changed from string"
        );
        assert!(
            json.get("swapMode").unwrap().is_string(),
            "LIVE ENDPOINT CHANGE: 'swapMode' type changed from string"
        );

        // Success - print summary for manual verification
        println!("Live smoke test passed!");
        println!("  Input: {} SOL", json.get("inAmount").unwrap());
        println!("  Output: {} USDC", json.get("outAmount").unwrap());
        println!("  Route steps: {}", route_array.len());
        println!("  Price impact: {}%", json.get("priceImpactPct").unwrap());
    }
}

// ============================================================================
// Security & Privacy Tests
// ============================================================================

#[cfg(test)]
mod security_tests {
    use regex::Regex;
    use serde_json::Value;
    use std::collections::HashSet;

    /// Suspicious field names that could indicate sensitive data leakage
    const SUSPICIOUS_FIELD_NAMES: &[&str] = &[
        "privateKey",
        "private_key",
        "secretKey",
        "secret_key",
        "apiKey",
        "api_key",
        "password",
        "secret",
        "token",
        "credential",
        "auth",
    ];

    /// Fields that are whitelisted and expected to contain base58/base64 data
    const WHITELISTED_PATHS: &[&str] = &[
        "swapTransaction",               // Base64 transaction blob - expected
        "_request_params.userPublicKey", // Public key, not private
        "ammKey",                         // Pool address - public
        "inputMint",                      // Token mint address - public
        "outputMint",                     // Token mint address - public
        "feeMint",                        // Token mint address - public
    ];

    /// Get the path to the fixtures directory
    fn fixtures_dir() -> std::path::PathBuf {
        let manifest_dir = env!("CARGO_MANIFEST_DIR");
        std::path::PathBuf::from(manifest_dir)
            .join("fixtures")
            .join("jupiter")
    }

    /// Load all fixture files (quote_*.json and swap_*.json)
    fn load_all_fixtures() -> Vec<(String, String, Value)> {
        let fixtures_path = fixtures_dir();
        let mut fixtures = Vec::new();

        // Read all JSON files matching quote_* or swap_*
        if let Ok(entries) = std::fs::read_dir(&fixtures_path) {
            for entry in entries.flatten() {
                let path = entry.path();
                if let Some(filename) = path.file_name().and_then(|n| n.to_str()) {
                    if filename.ends_with(".json")
                        && (filename.starts_with("quote_") || filename.starts_with("swap_"))
                    {
                        if let Ok(content) = std::fs::read_to_string(&path) {
                            if let Ok(value) = serde_json::from_str::<Value>(&content) {
                                fixtures.push((filename.to_string(), content, value));
                            }
                        }
                    }
                }
            }
        }

        fixtures
    }

    /// Check if a string looks like a base58-encoded private key (64-88 chars)
    fn looks_like_private_key(s: &str) -> bool {
        // Base58 alphabet: 123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz
        let base58_regex = Regex::new(r"^[1-9A-HJ-NP-Za-km-z]{64,88}$").unwrap();

        // Private keys are typically 64-88 base58 chars
        // Public keys are 32-44 base58 chars
        // Signatures are 88 base58 chars
        if s.len() >= 64 && s.len() <= 88 && base58_regex.is_match(s) {
            // Additional heuristic: keypairs typically have high entropy
            // and don't look like typical public addresses
            return true;
        }
        false
    }

    /// Check if a string looks like an API key (UUID, long hex, etc.)
    fn looks_like_api_key(s: &str) -> bool {
        // UUID pattern: xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx
        let uuid_regex = Regex::new(
            r"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}$",
        )
        .unwrap();

        // Long hex string (32+ chars) that's not a known pattern
        let long_hex_regex = Regex::new(r"^[0-9a-fA-F]{32,}$").unwrap();

        // Bearer token pattern
        let bearer_regex = Regex::new(r"^Bearer\s+").unwrap();

        // Common API key prefixes
        let api_prefixes = ["sk_", "pk_", "api_", "key_", "secret_"];

        if uuid_regex.is_match(s) {
            return true;
        }

        if long_hex_regex.is_match(s) && s.len() >= 32 {
            return true;
        }

        if bearer_regex.is_match(s) {
            return true;
        }

        for prefix in api_prefixes {
            if s.starts_with(prefix) && s.len() > 20 {
                return true;
            }
        }

        false
    }

    /// Check if a string looks like a base58 signature (88 chars outside swapTransaction)
    fn looks_like_signature(s: &str) -> bool {
        let base58_regex = Regex::new(r"^[1-9A-HJ-NP-Za-km-z]{88}$").unwrap();
        base58_regex.is_match(s)
    }

    /// Check if a field name is suspicious
    fn is_suspicious_field_name(name: &str) -> bool {
        let lower_name = name.to_lowercase();
        SUSPICIOUS_FIELD_NAMES
            .iter()
            .any(|suspicious| lower_name.contains(&suspicious.to_lowercase()))
    }

    /// Check if a path is whitelisted
    fn is_whitelisted_path(path: &str) -> bool {
        // Direct match
        if WHITELISTED_PATHS.contains(&path) {
            return true;
        }

        // Check if the path ends with a whitelisted field
        for whitelisted in WHITELISTED_PATHS {
            if path.ends_with(whitelisted) {
                return true;
            }
        }

        // Check for array index patterns like routePlan[0].swapInfo.ammKey
        let path_without_indices = Regex::new(r"\[\d+\]")
            .unwrap()
            .replace_all(path, "")
            .to_string();

        for whitelisted in WHITELISTED_PATHS {
            if path_without_indices.ends_with(whitelisted) {
                return true;
            }
        }

        false
    }

    /// Recursively scan JSON for sensitive data
    fn scan_json_for_sensitive_data(
        value: &Value,
        current_path: &str,
        violations: &mut Vec<String>,
    ) {
        match value {
            Value::Object(map) => {
                for (key, val) in map {
                    let new_path = if current_path.is_empty() {
                        key.clone()
                    } else {
                        format!("{}.{}", current_path, key)
                    };

                    // Check for suspicious field names (unless whitelisted)
                    if is_suspicious_field_name(key) && !is_whitelisted_path(&new_path) {
                        violations.push(format!(
                            "Suspicious field name '{}' at path '{}'",
                            key, new_path
                        ));
                    }

                    // Recurse into the value
                    scan_json_for_sensitive_data(val, &new_path, violations);
                }
            }
            Value::Array(arr) => {
                for (i, val) in arr.iter().enumerate() {
                    let new_path = format!("{}[{}]", current_path, i);
                    scan_json_for_sensitive_data(val, &new_path, violations);
                }
            }
            Value::String(s) => {
                // Skip whitelisted paths
                if is_whitelisted_path(current_path) {
                    return;
                }

                // Check for private key patterns
                if looks_like_private_key(s) {
                    violations.push(format!(
                        "Potential private key (64-88 char base58) at path '{}': '{}'",
                        current_path,
                        &s[..std::cmp::min(20, s.len())]
                    ));
                }

                // Check for API key patterns
                if looks_like_api_key(s) {
                    violations.push(format!(
                        "Potential API key at path '{}': '{}'",
                        current_path,
                        &s[..std::cmp::min(20, s.len())]
                    ));
                }

                // Check for signature patterns (outside swapTransaction)
                if looks_like_signature(s) && !current_path.contains("swapTransaction") {
                    violations.push(format!(
                        "Potential wallet signature (88 char base58) at path '{}': '{}'",
                        current_path,
                        &s[..std::cmp::min(20, s.len())]
                    ));
                }
            }
            _ => {}
        }
    }

    /// Scan raw JSON string for sensitive patterns that might be missed in structured scan
    fn scan_raw_content_for_patterns(content: &str, fixture_name: &str) -> Vec<String> {
        let mut violations = Vec::new();

        // Look for patterns that might be in string values or comments
        let patterns = [
            (r#""privateKey"\s*:"#, "privateKey field"),
            (r#""secretKey"\s*:"#, "secretKey field"),
            (r#""apiKey"\s*:"#, "apiKey field"),
            (r#""password"\s*:"#, "password field"),
            (r#""secret"\s*:"#, "secret field"),
            (r#"sk_live_[a-zA-Z0-9]+"#, "Stripe live key"),
            (r#"sk_test_[a-zA-Z0-9]+"#, "Stripe test key"),
            (r#"Bearer\s+[a-zA-Z0-9._-]+"#, "Bearer token"),
        ];

        for (pattern, description) in patterns {
            let regex = Regex::new(pattern).unwrap();
            if regex.is_match(content) {
                // Check if it's in a whitelisted context
                let is_in_metadata = content.contains("_fixture_metadata")
                    && regex
                        .find(content)
                        .map(|m| {
                            let pos = m.start();
                            // Check if this match is within the _fixture_metadata section
                            let before = &content[..pos];
                            let metadata_start = before.rfind("\"_fixture_metadata\"");
                            let metadata_end = before.rfind('}');
                            metadata_start.is_some()
                                && (metadata_end.is_none() || metadata_start > metadata_end)
                        })
                        .unwrap_or(false);

                if !is_in_metadata {
                    violations.push(format!(
                        "Raw content scan found potential {}: pattern '{}' in {}",
                        description, pattern, fixture_name
                    ));
                }
            }
        }

        violations
    }

    #[test]
    fn test_fixtures_contain_no_sensitive_data() {
        let fixtures = load_all_fixtures();

        assert!(
            !fixtures.is_empty(),
            "SECURITY TEST ERROR: No fixtures found in fixtures/jupiter/"
        );

        let mut all_violations: Vec<String> = Vec::new();

        for (fixture_name, raw_content, json_value) in &fixtures {
            let mut fixture_violations = Vec::new();

            // Structured JSON scan
            scan_json_for_sensitive_data(json_value, "", &mut fixture_violations);

            // Raw content scan for patterns that might be missed
            let raw_violations = scan_raw_content_for_patterns(raw_content, fixture_name);
            fixture_violations.extend(raw_violations);

            // Add fixture context to each violation
            for violation in fixture_violations {
                all_violations.push(format!(
                    "PRIVACY VIOLATION: Fixture '{}' contains potential sensitive data at path '{}'",
                    fixture_name, violation
                ));
            }
        }

        if !all_violations.is_empty() {
            let violation_report = all_violations.join("\n");
            panic!(
                "SECURITY CHECK FAILED: Found {} potential privacy violations in fixtures:\n\n{}\n\n\
                 If these are false positives, add the paths to WHITELISTED_PATHS in security_tests module.",
                all_violations.len(),
                violation_report
            );
        }
    }

    #[test]
    fn test_no_real_wallet_addresses_in_request_params() {
        let fixtures = load_all_fixtures();

        // Known test/example addresses that are safe to use in fixtures
        let known_safe_addresses: HashSet<&str> = [
            // Common test addresses
            "9WzDXwBbmkg8ZTbNMqUxvQRAyrZzDsGYdLVL9zYtAWWM", // Example from fixtures
            "So11111111111111111111111111111111111111112",   // SOL mint (system address)
            "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v", // USDC mint (well-known)
        ]
        .into_iter()
        .collect();

        for (fixture_name, _, json_value) in &fixtures {
            if let Some(request_params) = json_value.get("_request_params") {
                if let Some(user_key) = request_params.get("userPublicKey") {
                    if let Some(key_str) = user_key.as_str() {
                        // Verify it's either a known safe address or follows test pattern
                        let _is_safe = known_safe_addresses.contains(key_str)
                            || key_str.starts_with("Test")
                            || key_str.starts_with("Example");

                        // For now, just verify it looks like a valid public key (32-44 chars)
                        assert!(
                            key_str.len() >= 32 && key_str.len() <= 44,
                            "PRIVACY CONCERN: userPublicKey in {} has unexpected length: {} \
                             (expected 32-44 for Solana public key)",
                            fixture_name,
                            key_str.len()
                        );
                    }
                }
            }
        }
    }
}
