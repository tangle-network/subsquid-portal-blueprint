use blueprint_sdk::runner::config::BlueprintEnvironment;
use blueprint_sdk::tangle::layers::TangleLayer;
use blueprint_sdk::tangle::serde::from_field;
use blueprint_sdk::testing::tempfile;
use blueprint_sdk::testing::utils::setup_log;
use blueprint_sdk::testing::utils::tangle::TangleTestHarness;
use blueprint_sdk::{Job, tangle::serde::to_field};
use std::fs;
use std::path::PathBuf;
use subsquid_portal_blueprint_lib::{
    PROVISION_PORTAL_JOB_ID, SubsquidPortalContext, provision_portal,
};

// The number of nodes to spawn in the test
const N: usize = 1;

#[tokio::test]
#[ignore]
async fn test_provision_portal_e2e() -> color_eyre::Result<()> {
    setup_log();

    // Initialize test harness (node, keys, deployment)
    let temp_dir = tempfile::TempDir::new()?;

    // Manually create the blueprint environment within the temp dir for context init
    let mock_env_path = temp_dir.path().join("mock-env");
    fs::create_dir_all(&mock_env_path)?;
    let env = BlueprintEnvironment::default();

    // Initialize the context manually for the harness
    let context = SubsquidPortalContext::new(env.clone()).await?;

    // --- Setup Test Dependencies ---
    // Create a dummy key file for testing within the context's data dir
    let key_dir = context.base_data_dir.join("keys");
    tokio::fs::create_dir_all(&key_dir).await?;
    let dummy_key_path = key_dir.join("test_network.key");
    tokio::fs::write(&dummy_key_path, "dummy_key_content").await?;

    // Copy the docker-compose file into the context's data dir
    let compose_source =
        PathBuf::from("../subsquid-portal-blueprint-lib/subsquid-docker-compose.yml"); // Adjust path relative to test execution dir
    let compose_dest = context.base_data_dir.join("subsquid-docker-compose.yml");
    if !compose_source.exists() {
        panic!(
            "Source compose file not found at: {}",
            compose_source.display()
        );
    }
    tokio::fs::copy(&compose_source, &compose_dest).await?;
    // --- End Test Dependencies Setup ---

    let harness = TangleTestHarness::setup(temp_dir).await?;

    // Setup service with `N` nodes
    let (mut test_env, service_id, _) = harness.setup_services::<N>(false).await?;

    // Setup the node(s) - Add the provision_portal job
    test_env.initialize().await?;
    test_env.add_job(provision_portal.layer(TangleLayer)).await;

    // Start the test environment.
    test_env.start(context).await?;

    // Prepare job inputs
    let network = "tethys".to_string();
    let key_path_str = dummy_key_path.to_str().unwrap().to_string();
    let rpc_url = "http://localhost:8545".to_string(); // Mock RPC
    let l1_rpc_url: Option<String> = None;
    let boot_nodes: Option<Vec<String>> = None;

    let job_inputs = vec![
        to_field(network)?,
        to_field(key_path_str)?,
        to_field(rpc_url)?,
        to_field(l1_rpc_url)?,
        to_field(boot_nodes)?,
    ];

    // Submit the job call (use the correct job ID)
    let job = harness
        .submit_job(service_id, PROVISION_PORTAL_JOB_ID, job_inputs)
        .await?;

    // Wait for job execution
    let results = harness.wait_for_job_execution(service_id, job).await?;

    // Verify results (check for success message substring)
    let output_field = results.result.get(0).expect("Job should have one output");
    let result_string: String = from_field(output_field.clone())?;

    println!("Provisioning result: {}", result_string);
    assert!(
        result_string.contains("Successfully provisioned portal. Container ID:"),
        "Job output did not contain success message"
    );
    assert_eq!(results.service_id, service_id);

    // TODO: Add Docker cleanup logic if possible/needed within the test harness context
    // Might involve calling docker stop/rm on the container ID extracted from result_string

    Ok(())
}
