-- M4: keep the project label on saved timesheet lines ("Meetings", or a project that was later renamed).
ALTER TABLE timesheet_entries ADD COLUMN project_name TEXT;
