# Changelog

## 🎉 [0.3.0] - 2024-06-14

### ✨ Features

- Added loading of rest days for the correct operation of the sum command
- Added average operating time to the sum command report
- Added gitlab api module
- Added commits from gitlab to create a list of tasks

### 🛠️ Bug Fixes

- Fixed a bug with receiving commits from Gitlab

## 🎉 [0.2.0] - 2024-05-17

### ✨ Features

- Watch command has been added
- Added "raw" flag to the "event" command

### 🛠️ Bug Fixes

- Fixed a bug in calculating time using the "sum" command. Optimization of the events db module.

## 🎉 [0.1.1] - 2024-04-08

### ✨ Features

- Added aliases for event command

### 🛠️ Bug Fixes

- The path to SESSION_ID_FILE has been corrected in the delete_session_id method
- The service println has been removed
- Fixed an error in calculating time if the interval end timestamp is missed

## 🎉 [0.1.0] - 2024-03-25

### ✨ Features

- A simple wizard has been added to set configuration settings
- Summary command has been added

### 🛠️ Bug Fixes

- Unused commands have been removed
- The final daily event has been added after submitting report

## 🎉 [0.0.2] - 2024-03-19

### ✨ Features

- Bash installation script has been slightly modified

### 🛠️ Bug Fixes

- The path to the .session_id file has been replaced with the path in DataStorage

## 🎉 [0.0.1] - 2024-03-18

### ✨ Features

- Added basic commands
- The "rusqlite" library has been added to the project
- The "insert_event" method has been added to the Db module
- Task structure, tasks scheme and "insert_task" method has been added
- The "fetch" method has been added to the Tasks DB module
- The "excluded_from_search" and "task_id" fields have been added to the Task structure
- The "dialoguer" library been added to the project
- Finding unfinished tasks has recently been added as an option to the "task" command
- Data output in tabular form has been added to the task module
- Scheduler module has been added to the project
- The function for deleting tasks from the Windows Scheduler has been added to the project
- Events module has been updated
- Scheduler module has been updated
- The function of counting and displaying working time tables has been addded to the events module
- Working hours have been added to the events module
- Http module has been added
- Config module has been added
- Report command has been added
- Init command has been updated
- Report command has been updated
- The Http module was renamed to Si and moved to the api directory. Methods for working with the session storage file have also been added. The application configuration has been changed to accommodate the new requirements.
- Bash installation script has been added
- Created directory structure in AppData for database, user files and configuration.
- Bash installation script has been moved to the tools directory

### 🎛️ Refactor

- Database modules are moved to separate files
- The FormatEvents trait and FormatEvent structure have been added to the Event module
- The FormatTasks trait has been added to the Task module

### 🎲 Miscellaneous tasks

- Added github "release" action
- The .gitignore file has been updated

### 📖 Documentation

- Files added: README.md, LICENSE, rellr.json
- Kasl logo added
- A detailed user guide has been created
- Readmes and introductory documentation have been updated

### 🛠️ Bug Fixes

- Solved the error: "recursion in an `async fn` requires boxing"
- Renaming paths in kasl
- The input field has been replaced with a password entry
- The formation of the final report for sending has been changed

