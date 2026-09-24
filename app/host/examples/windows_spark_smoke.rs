#[cfg(windows)]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use easy_codex_host::rollout_observer::TurnPack;
    use easy_codex_host::spark_runner::{SparkRunner, SparkRunnerConfig};
    use easy_codex_host::store::{PendingSummaryCompletion, SummaryClaim, SummaryClaimOutcome};

    let task_id = "00000000-0000-4000-8000-000000000001".to_owned();
    let completion_id = "00000000-0000-4000-8000-000000000002".to_owned();
    let pack = TurnPack {
        v: 1,
        turn_id: completion_id.clone(),
        user: vec![],
        assistant: vec!["已为演示项目创建一个 Windows 原生 Host，状态接口返回 ready。".into()],
        tools: vec![],
    };
    let claim = SummaryClaim {
        outcome: SummaryClaimOutcome::Inserted,
        request_id: "windows-spark-smoke".into(),
        task_id,
        generation: 1,
        previous_unread: None,
        completions: vec![PendingSummaryCompletion {
            completion_id: completion_id.clone(),
            turn_pack: serde_json::to_string(&pack)?,
        }],
    };
    let runner = SparkRunner::new(SparkRunnerConfig::default());
    let document = runner.run(&claim, None)?;
    println!("status=ready");
    println!("coverage={}", document.covers_new_completions.len());
    println!("spoken_text={}", document.spoken_text);
    Ok(())
}

#[cfg(not(windows))]
fn main() {}
