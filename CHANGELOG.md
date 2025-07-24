# Changelog

## 🎉 [0.7.0] - 2025-07-24

### ✨ Features

- Add optional --month flag to generate report for current month
- Add activity monitoring daemon with enigo
- Add breaks table and CRUD operations
- Implement real activity detection
- Add kasl breaks command
- Add break monitoring and server configuration
- Add workdays table with basic CRUD operations
- Automate workday start detection in Monitor based on activity threshold
- Refactor 'sum' command to use workday/break model
- Refactor report command and remove events system
- Centralize formatting logic and improve documentation
- Implement background daemon mode
- Add total pause time calculation and display
- Add work productivity calculation and display
- Implement automatic restart on new instance
- Add application icon for Windows
- Add anyhow for better error handling
- Add centralized messages system
- Implement centralized messaging system and begin migration

### 🎛️ Refactor

- Remove system event-based scheduler
- Change start and end to TIMESTAMP in breaks table
- Refactor state management and activity detection
- Improve report command and table output
- Rename 'Break' entity to 'Pause'
- Refactor update logic and improve command structure
- Migrate database modules to anyhow
- Migrate entire codebase to anyhow for better error handling
- Migrate api and commands to new messaging system
- Migrate all messages to new messaging system

### 🛠️ Bug Fixes

- Fixed a bug with password request after session expires in Jira
- Fixed bug with requesting unavailable Jira API
- Refine activity detection with rdev
- Improve network error handling to prevent crashes
- Align sent payload with displayed report logic
- Correct visibility and imports to fix build errors
- Adjust pause start time by threshold value
- Fix Windows process termination and refactor daemon logic
- Embed encryption keys at compile time

### 🧪 Testing

- Add tests for config
- Tests for report have been fixed

## 🎉 [0.6.0] - 2024-08-01

### ✨ Features

- Added Secret module for encryption and decryption of passwords to services
- Secret module functionality added to Jira service
- Secret module functionality added to Si service
- Added update checking functionality and a command to update the application to the latest version

### 🎛️ Refactor

- Common methods are moved to the Session trait
- Improved functionality of build.rs

### 🛠️ Bug Fixes

- Added message that update is not required if the latest version is used

## 🎉 [0.5.0] - 2024-07-11

### ✨ Features

- Improved view of the list of issues proposed for adding (divided into groups: Incomplete, Gitlab, Jira, etc.)
- Tasks are divided into groups for even distribution in the report

### 🛠️ Bug Fixes

- The current day is excluded from the sum command's calculation of monthly statistics

## 🎉 [0.4.1] - 2024-06-30

### ✨ Features

- Added a function for sending a monthly summary report in the Si module

### 🛠️ Bug Fixes

- Removed information about commit ID from Gitlab

## 🎉 [0.4.0] - 2024-06-25

### ✨ Features

- Added functionality to create a report for a specific date
- Added Jira API module
- Added tasks from Jira to fill the list of daily tasks

### 🎛️ Refactor

- API module configs have been moved from the libs/config.rs file to the corresponding module files

### 🎲 Miscellaneous tasks

- Reqwest library updated
- The .session_id file has been renamed to .si_session_id

### 🛠️ Bug Fixes

- Fixed a bug in searching tasks by ID

## 🎉 [0.3.0] - 2024-06-18

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

