use std::env;
use windows::core::{ComInterface, Result, BSTR};
use windows::Win32::Foundation::VARIANT_BOOL;
use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CLSCTX_ALL, COINIT_MULTITHREADED};
use windows::Win32::System::TaskScheduler::{
    IAction, IActionCollection, IEventTrigger, IExecAction, IPrincipal, IRegisteredTask, ITaskDefinition, ITaskFolder, ITaskService, ITaskSettings, ITriggerCollection,
    TaskScheduler, TASK_ACTION_EXEC, TASK_CREATE_OR_UPDATE, TASK_LOGON_INTERACTIVE_TOKEN, TASK_RUNLEVEL_LUA, TASK_TRIGGER_EVENT,
};
use windows::Win32::System::Variant::VARIANT;

pub enum EventCode {
    Lock = 4800,
    Unlock = 4801,
    Start = 6005,
}

pub struct Scheduler {}
impl Scheduler {
    pub fn new() -> Result<()> {
        let command = "wflow";
        let current_exe_path = env::current_exe().unwrap();
        let current_dir_path = current_exe_path.parent().unwrap().to_str().unwrap();
        let start_action = TaskAction::new("action", &command, &current_dir_path, "event start");
        let end_action = TaskAction::new("action", &command, &current_dir_path, "event end");

        Task::new(r"\")?
            .event_trigger(EventCode::Start)?
            .exec_action(&start_action)?
            .principal("", "")?
            .set_hidden(true)?
            .register("wflow boot")?;

        Task::new(r"\")?
            .event_trigger(EventCode::Unlock)?
            .exec_action(&start_action)?
            .principal("", "")?
            .set_hidden(true)?
            .register("wflow start")?;

        Task::new(r"\")?
            .event_trigger(EventCode::Lock)?
            .exec_action(&end_action)?
            .principal("", "")?
            .set_hidden(true)?
            .register("wflow end")?;

        Ok(())
    }

    pub fn delete() -> Result<()> {
        Task::delete_task(r"\", "wflow boot")?;
        Task::delete_task(r"\", "wflow start")?;
        Task::delete_task(r"\", "wflow end")?;

        Ok(())
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

    pub fn event_trigger(self, event_code: EventCode) -> Result<Self> {
        let event_id = event_code as u32;
        let query = format!(
            "<QueryList><Query Id='0' Path='Security'><Select Path='Security'>*[System[EventID={}]]</Select></Query></QueryList>",
            &event_id
        );

        unsafe {
            let trigger = self.triggers.Create(TASK_TRIGGER_EVENT)?;
            let i_event_trigger = trigger.cast::<IEventTrigger>()?;
            i_event_trigger.SetId(&BSTR::from(event_id.to_string()))?;
            i_event_trigger.SetEnabled(VARIANT_BOOL(1))?;
            i_event_trigger.SetSubscription(&BSTR::from(query))?;
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

    pub fn exec_action(self, task_action: &TaskAction) -> Result<Self> {
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
