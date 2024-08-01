use crate::{
    api::{gitlab::GitLab, jira::Jira},
    db::tasks::Tasks,
    libs::{
        config::Config,
        task::{Task, TaskFilter},
        view::View,
    },
};
use chrono::Local;
use clap::Args;
use dialoguer::{theme::ColorfulTheme, Input, MultiSelect};
use std::error::Error;

#[derive(Debug, PartialEq, Eq, Hash)]
enum TaskSource {
    Incomplete,
    Gitlab,
    Jira,
}

#[derive(Debug, Args)]
pub struct TaskArgs {
    #[arg(short, long)]
    name: Option<String>,
    #[arg(long)]
    comment: Option<String>,
    #[arg(short, long)]
    completeness: Option<i32>,
    #[arg(short, long)]
    show: bool,
    #[arg(short, long)]
    all: bool,
    #[arg(short, long)]
    id: Option<Vec<i32>>,
    #[arg(short, long, help = "Find incomplete tasks")]
    find: bool,
}

pub async fn cmd(task_args: TaskArgs) -> Result<(), Box<dyn Error>> {
    let date = Local::now();
    if task_args.show {
        let mut filter: TaskFilter = TaskFilter::Date(date.date_naive());
        if task_args.all {
            filter = TaskFilter::All;
        } else if task_args.id.is_some() {
            filter = TaskFilter::ByIds(task_args.id.unwrap());
        }
        let tasks = Tasks::new()?.fetch(filter)?;
        if tasks.is_empty() {
            println!("Tasks not found((");
            return Ok(());
        }
        View::tasks(&tasks)?;

        return Ok(());
    } else if task_args.find {
        // Incomplete tasks
        let mut tasks: Vec<(&TaskSource, Vec<Task>)> = Vec::new();
        let incomplete_tasks = Tasks::new()?.fetch(TaskFilter::Incomplete)?;
        if !incomplete_tasks.is_empty() {
            tasks.push((&TaskSource::Incomplete, incomplete_tasks));
        }

        let config = Config::read()?;
        // Gitlab commits
        if config.gitlab.is_some() {
            let today_tasks = Tasks::new()?.fetch(TaskFilter::Date(date.date_naive()))?;
            let commits = GitLab::new(&config.gitlab.unwrap()).get_today_commits().await?;
            let mut gitlab_tasks: Vec<Task> = Vec::new();
            commits.iter().for_each(|commit| {
                if today_tasks.iter().all(|task| task.name != commit.message) {
                    gitlab_tasks.push(Task::new(&commit.message, "", Some(100)));
                }
            });
            if !gitlab_tasks.is_empty() {
                tasks.push((&TaskSource::Gitlab, gitlab_tasks));
            }
        }
        // Jira issues
        if config.jira.is_some() {
            let jira_issues = Jira::new(&config.jira.unwrap()).get_completed_issues(&date.date_naive()).await?;
            let mut jira_tasks: Vec<Task> = Vec::new();
            jira_issues.iter().for_each(|issue| {
                let name = format!("{} {}", &issue.key, &issue.fields.summary);
                jira_tasks.push(Task::new(&name, "", Some(100)));
            });
            if !jira_tasks.is_empty() {
                tasks.push((&TaskSource::Jira, jira_tasks));
            }
        }

        if tasks.iter().all(|(_, task)| task.is_empty()) {
            println!("Tasks not found((");
            return Ok(());
        }

        let mut selected_tasks: Vec<(&TaskSource, Vec<usize>)> = Vec::new();
        for (task_source, tasks) in tasks.iter() {
            let mut name_format: Box<dyn Fn(&Task) -> String> = Box::new(|task: &Task| task.name.to_owned());
            match task_source {
                TaskSource::Incomplete => {
                    println!("\nIncomplete tasks");
                    name_format = Box::new(|task: &Task| format!("{} - {}%", task.name, task.completeness.unwrap_or(0)));
                }
                TaskSource::Gitlab => println!("\nGitlab commits"),
                TaskSource::Jira => println!("\nJira issues"),
            }
            let task_names: Vec<String> = tasks.iter().map(name_format).collect();
            selected_tasks.push((
                task_source,
                MultiSelect::with_theme(&ColorfulTheme::default())
                    .with_prompt("Select options")
                    .items(&task_names)
                    .interact()
                    .unwrap(),
            ));
        }

        for (task_source, selected_task_indexes) in selected_tasks {
            for index in selected_task_indexes {
                let mut task = tasks.iter().find(|(ts, _)| ts == &task_source).map_or(&vec![], |(_, tasks)| tasks)[index].clone();
                match task_source {
                    TaskSource::Incomplete => {
                        println!("Selected task: {}", &task.name);
                        if task.task_id.is_none() || task.task_id.is_some_and(|id| id == 0) {
                            task.task_id = task.id;
                        }
                        let default_completeness = (task.completeness.unwrap() + 1).min(100);
                        task.completeness = Some(
                            Input::with_theme(&ColorfulTheme::default())
                                .allow_empty(true)
                                .with_prompt("Enter completeness")
                                .default(default_completeness)
                                .interact_text()
                                .unwrap(),
                        );
                    }
                    _ => {}
                }
                let _ = Tasks::new()?.insert(&task);
            }
        }

        return Ok(());
    }

    let name = task_args.name.unwrap_or_else(|| {
        Input::with_theme(&ColorfulTheme::default())
            .with_prompt("Enter task name")
            .interact_text()
            .unwrap()
    });
    let comment = task_args.comment.unwrap_or_else(|| {
        Input::with_theme(&ColorfulTheme::default())
            .allow_empty(true)
            .with_prompt("Enter comment")
            .interact_text()
            .unwrap()
    });
    let completeness = task_args.completeness.unwrap_or_else(|| {
        Input::with_theme(&ColorfulTheme::default())
            .allow_empty(true)
            .with_prompt("Enter completeness")
            .default(100)
            .interact_text()
            .unwrap()
    });

    let task = Task::new(&name, &comment, Some(completeness));
    let new_task = Tasks::new()?.insert(&task)?.update_id()?.get()?;
    View::tasks(&new_task)?;

    Ok(())
}
