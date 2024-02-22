use std::time::Duration;
use windows::core::{ComInterface, Result, BSTR};
use windows::Win32::Foundation::VARIANT_BOOL;
use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CLSCTX_ALL, COINIT_MULTITHREADED};
use windows::Win32::System::TaskScheduler::{
    IAction, IActionCollection, IExecAction, ILogonTrigger, IPrincipal, IRegisteredTask, ITaskDefinition, ITaskFolder, ITaskService, ITaskSettings, ITriggerCollection,
    TaskScheduler, TASK_ACTION_EXEC, TASK_CREATE_OR_UPDATE, TASK_LOGON_INTERACTIVE_TOKEN, TASK_RUNLEVEL_LUA, TASK_TRIGGER_LOGON,
};
use windows::Win32::System::Variant::VARIANT;

pub struct Scheduler {}
impl Scheduler {
    pub fn new() -> Result<()> {
        let logon_trigger = TaskLogonTrigger::new("logontrigger", Duration::from_secs(3 * 60), true, Duration::from_secs(10), Duration::from_secs(1));

        let action = TaskAction::new("action", "notepad.exe", "", "");

        Task::new(r"\")?
            .logon_trigger(logon_trigger)?
            .exec_action(action)?
            .principal("", "")?
            .set_hidden(false)?
            .register("logon_trigger")?;

        Ok(())
    }

    pub fn delete() -> Result<()> {
        Task::delete_task(r"\", "logon_trigger")?;

        Ok(())
    }
}

pub struct TaskLogonTrigger {
    pub(crate) id: BSTR,
    pub(crate) repetition_interval: BSTR,
    pub(crate) repetition_stop_at_duration_end: i16,
    pub(crate) execution_time_limit: BSTR,
    pub(crate) delay: BSTR,
}
impl TaskLogonTrigger {
    pub fn new(id: &str, repetition_interval: Duration, repetition_stop_at_duration_end: bool, execution_time_limit: Duration, delay: Duration) -> Self {
        Self {
            id: id.into(),
            repetition_interval: format!("PT{}S", repetition_interval.as_secs()).into(),
            repetition_stop_at_duration_end: repetition_stop_at_duration_end as i16,
            execution_time_limit: format!("PT{}S", execution_time_limit.as_secs()).into(),
            delay: format!("PT{}S", delay.as_secs()).into(),
        }
    }
}

pub struct TaskAction {
    pub(crate) id: BSTR,
    pub(crate) path: BSTR,
    pub(crate) working_dir: BSTR,
    pub(crate) args: BSTR,
}
impl TaskAction {
    pub fn new(id: &str, path: &str, working_dir: &str, args: &str) -> Self {
        Self {
            id: id.into(),
            path: path.into(),
            working_dir: working_dir.into(),
            args: args.into(),
        }
    }
}

pub struct Task {
    task_definition: ITaskDefinition,
    triggers: ITriggerCollection,
    actions: IActionCollection,
    settings: ITaskSettings,
    folder: ITaskFolder,
}
impl Task {
    fn get_task_service() -> Result<ITaskService> {
        unsafe {
            CoInitializeEx(Some(std::ptr::null_mut()), COINIT_MULTITHREADED)?;

            let task_service: ITaskService = CoCreateInstance(&TaskScheduler, None, CLSCTX_ALL)?;
            task_service.Connect(VARIANT::default(), VARIANT::default(), VARIANT::default(), VARIANT::default())?;
            Ok(task_service)
        }
    }

    pub fn new(path: &str) -> Result<Self> {
        unsafe {
            let task_service = Self::get_task_service()?;

            let task_definition: ITaskDefinition = task_service.NewTask(0)?;
            let triggers: ITriggerCollection = task_definition.Triggers()?;
            let actions: IActionCollection = task_definition.Actions()?;
            let settings: ITaskSettings = task_definition.Settings()?;
            let folder: ITaskFolder = task_service.GetFolder(&BSTR::from(path))?;

            Ok(Self {
                task_definition,
                triggers,
                actions,
                settings,
                folder,
            })
        }
    }

    pub fn register(self, name: &str) -> Result<IRegisteredTask> {
        unsafe {
            let registered_task = self.folder.RegisterTaskDefinition(
                &BSTR::from(name),
                &self.task_definition,
                TASK_CREATE_OR_UPDATE.0,
                VARIANT::default(),
                VARIANT::default(),
                TASK_LOGON_INTERACTIVE_TOKEN,
                VARIANT::default(),
            )?;
            self.settings.SetEnabled(VARIANT_BOOL(1))?;
            Ok(registered_task)
        }
    }

    pub fn set_hidden(self, is_hidden: bool) -> Result<Self> {
        unsafe { self.settings.SetHidden(VARIANT_BOOL(is_hidden as i16))? }
        Ok(self)
    }

    pub fn logon_trigger(self, logon_trigger: TaskLogonTrigger) -> Result<Self> {
        unsafe {
            let trigger = self.triggers.Create(TASK_TRIGGER_LOGON)?;
            let i_logon_trigger = trigger.cast::<ILogonTrigger>()?;
            i_logon_trigger.SetId(&logon_trigger.id)?;
            i_logon_trigger.SetEnabled(VARIANT_BOOL(1))?;
            i_logon_trigger.SetExecutionTimeLimit(&logon_trigger.execution_time_limit)?;

            let repetition = i_logon_trigger.Repetition()?;
            repetition.SetInterval(&logon_trigger.repetition_interval)?;
            repetition.SetStopAtDurationEnd(VARIANT_BOOL(logon_trigger.repetition_stop_at_duration_end))?;

            i_logon_trigger.SetDelay(&logon_trigger.delay)?;
        }
        Ok(self)
    }

    pub fn principal(self, id: &str, user_id: &str) -> Result<Self> {
        unsafe {
            let principal: IPrincipal = self.task_definition.Principal()?;
            principal.SetRunLevel(TASK_RUNLEVEL_LUA)?;
            principal.SetId(&BSTR::from(id))?;
            principal.SetUserId(&BSTR::from(user_id))?;
        }
        Ok(self)
    }

    pub fn exec_action(self, task_action: TaskAction) -> Result<Self> {
        unsafe {
            let action: IAction = self.actions.Create(TASK_ACTION_EXEC)?;
            let exec_action: IExecAction = action.cast()?;

            exec_action.SetPath(&task_action.path)?;
            exec_action.SetId(&task_action.id)?;
            exec_action.SetWorkingDirectory(&task_action.working_dir)?;
            exec_action.SetArguments(&task_action.args)?;
        }
        Ok(self)
    }

    pub fn delete_task(path: &str, name: &str) -> Result<()> {
        unsafe {
            let task_service = Self::get_task_service()?;
            let folder = task_service.GetFolder(&BSTR::from(path))?;
            folder.DeleteTask(&BSTR::from(name), 0)?;
        }
        Ok(())
    }
}
