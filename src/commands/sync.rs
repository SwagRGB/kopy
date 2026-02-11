//! Main sync command

use crate::diff::generate_sync_plan;
use crate::executor::{execute_plan, ExecutionEvent};
use crate::scanner::scan_directory;
use crate::types::KopyError;
use crate::Config;

/// Run the sync operation
pub fn run(config: Config) -> Result<(), KopyError> {
    println!("Scanning source: {}", config.source.display());
    let src_tree = scan_directory(&config.source, &config, None)?;

    println!("Scanning destination: {}", config.destination.display());
    let dest_tree = if config.destination.exists() {
        scan_directory(&config.destination, &config, None)?
    } else {
        crate::types::FileTree::new(config.destination.clone())
    };

    let plan = generate_sync_plan(&src_tree, &dest_tree, &config);
    print_plan_summary(&plan);

    if config.dry_run {
        println!("Dry-run mode: no changes were made.");
        return Ok(());
    }

    if plan.actions.is_empty() {
        println!("Nothing to sync.");
        return Ok(());
    }

    let progress = |event: &ExecutionEvent| match event {
        ExecutionEvent::ActionStart {
            index,
            total,
            action,
            path,
        } => {
            let path_display = path
                .as_ref()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|| "<none>".to_string());
            println!("[{}/{}] {} {}", index, total, action, path_display);
        }
        ExecutionEvent::ActionSuccess {
            index,
            total,
            action,
            path,
            bytes_copied,
        } => {
            let path_display = path
                .as_ref()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|| "<none>".to_string());
            if *bytes_copied > 0 {
                println!(
                    "[{}/{}] OK {} {} ({} bytes)",
                    index, total, action, path_display, bytes_copied
                );
            } else {
                println!("[{}/{}] OK {} {}", index, total, action, path_display);
            }
        }
        ExecutionEvent::ActionError {
            index,
            total,
            action,
            path,
            error,
        } => {
            let path_display = path
                .as_ref()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|| "<none>".to_string());
            println!(
                "[{}/{}] ERROR {} {}: {}",
                index, total, action, path_display, error
            );
        }
        ExecutionEvent::Complete { stats } => {
            println!(
                "Execution complete: {} succeeded, {} failed, {} bytes copied.",
                stats.completed_actions, stats.failed_actions, stats.bytes_copied
            );
        }
    };

    execute_plan(&plan, &config, Some(&progress))?;
    Ok(())
}

fn print_plan_summary(plan: &crate::diff::DiffPlan) {
    println!("Plan:");
    println!(
        "  Copy: {}  Update: {}  Delete: {}  Skip: {}",
        plan.stats.copy_count,
        plan.stats.overwrite_count,
        plan.stats.delete_count,
        plan.stats.skip_count
    );
    println!("  Total bytes to transfer: {}", plan.stats.total_bytes);

    for action in &plan.actions {
        match action {
            crate::types::SyncAction::CopyNew(entry) => {
                println!("  COPY      {}", entry.path.display());
            }
            crate::types::SyncAction::Overwrite(entry) => {
                println!("  UPDATE    {}", entry.path.display());
            }
            crate::types::SyncAction::Delete(path) => {
                println!("  DELETE    {}", path.display());
            }
            crate::types::SyncAction::Skip => {
                println!("  SKIP      <unchanged>");
            }
            crate::types::SyncAction::Move { from, to } => {
                println!("  MOVE      {} -> {}", from.display(), to.display());
            }
        }
    }
}
