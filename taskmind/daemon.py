"""Main daemon loop - orchestrates capture at intervals."""
import os
import sys
import time
import signal
import logging
from datetime import datetime
from taskmind.config import load_config, PID_FILE, LOG_FILE, ensure_dirs
from taskmind.database import init_db, insert_activity
from taskmind.capture.window_tracker import get_active_window
from taskmind.capture.idle_detector import is_idle
from taskmind.processing.classifier import classify

logger = logging.getLogger("taskmind")
_running = True


def _setup_logging():
    ensure_dirs()
    logging.basicConfig(
        filename=LOG_FILE,
        level=logging.INFO,
        format="%(asctime)s [%(levelname)s] %(message)s",
        datefmt="%Y-%m-%d %H:%M:%S",
    )


def _write_pid():
    ensure_dirs()
    with open(PID_FILE, "w") as f:
        f.write(str(os.getpid()))


def _remove_pid():
    if os.path.exists(PID_FILE):
        os.remove(PID_FILE)


def _signal_handler(sig, frame):
    global _running
    _running = False


def is_daemon_running():
    """Check if daemon is currently running."""
    if not os.path.exists(PID_FILE):
        return False
    try:
        with open(PID_FILE, "r") as f:
            pid = int(f.read().strip())
        os.kill(pid, 0)  # Check if process exists
        return True
    except (OSError, ValueError):
        _remove_pid()
        return False


MEETING_KEYWORDS = ["Meet -", "Zoom Meeting", "Microsoft Teams", "meet.google.com", "Huddle"]
_in_meeting = False


def _is_meeting_window(title):
    """Check if window title indicates an active meeting."""
    if not title:
        return False
    return any(kw.lower() in title.lower() for kw in MEETING_KEYWORDS)


def run_daemon():
    """Main daemon entry point."""
    global _running, _in_meeting

    if is_daemon_running():
        print("TaskMind daemon is already running.")
        sys.exit(1)

    _setup_logging()
    init_db()
    _write_pid()

    signal.signal(signal.SIGINT, _signal_handler)
    signal.signal(signal.SIGTERM, _signal_handler)

    config = load_config()
    interval = config["general"]["tracking_interval_seconds"]
    idle_threshold = config["general"]["idle_threshold_minutes"]
    blacklisted_apps = [a.lower() for a in config["tracking"]["blacklisted_apps"]]

    logger.info("TaskMind daemon started (PID %d, interval %ds)", os.getpid(), interval)
    print("TaskMind daemon started (PID {})".format(os.getpid()))

    try:
        from taskmind.utils.scheduler import start_scheduler, stop_scheduler
        start_scheduler()
        logger.info("Scheduler started (recap, reminders)")
    except Exception as e:
        logger.warning("Scheduler failed to start: %s", e)

    try:
        while _running:
            try:
                timestamp = datetime.now().isoformat()
                idle = is_idle(idle_threshold)

                if idle:
                    insert_activity(timestamp, "", "", "", "", interval, is_idle=True)
                    # Stop recording if idle during meeting
                    if _in_meeting:
                        _in_meeting = False
                        from taskmind.capture.audio_recorder import is_recording, stop_recording
                        if is_recording():
                            stop_recording()
                            logger.info("Meeting ended (idle). Recording stopped.")
                else:
                    window = get_active_window()
                    app = window["app_name"].lower()

                    # Skip blacklisted apps
                    if app in blacklisted_apps:
                        time.sleep(interval)
                        continue

                    project = classify(window["window_title"], window["app_name"], window["window_class"], window.get("browser_url", ""))
                    insert_activity(
                        timestamp,
                        window["window_title"],
                        window["app_name"],
                        window["window_class"],
                        project,
                        interval,
                        is_idle=False,
                        browser_url=window.get("browser_url", ""),
                    )

                    # Auto-record meetings (checks ALL open windows, not just focused)
                    from taskmind.capture.audio_recorder import is_recording, start_recording, stop_recording
                    from taskmind.capture.window_tracker import has_meeting_window
                    meeting_active = has_meeting_window(MEETING_KEYWORDS)
                    if meeting_active:
                        if not _in_meeting:
                            _in_meeting = True
                            if not is_recording():
                                filepath = start_recording()
                                if filepath:
                                    from taskmind.database import insert_recording
                                    insert_recording(timestamp, filepath)
                                    from taskmind.utils.notifications import notify
                                    notify("🔴 Meeting Recording", "Auto-recording started")
                                    logger.info("Meeting detected. Recording started: %s", filepath)
                    else:
                        if _in_meeting:
                            _in_meeting = False
                            if is_recording():
                                result = stop_recording()
                                if result:
                                    from taskmind.database import update_recording
                                    update_recording(result[0], timestamp, result[1], None, "done")
                                    from taskmind.utils.notifications import notify
                                    notify("⏹ Meeting Ended", "Recording stopped ({}s)".format(result[1]))
                                    logger.info("Meeting ended. Recording stopped: %s", result[0])
            except Exception as e:
                import traceback
                logger.error("Capture error: %s\n%s", e, traceback.format_exc())

            time.sleep(interval)
    finally:
        try:
            from taskmind.utils.scheduler import stop_scheduler
            stop_scheduler()
        except Exception:
            pass
        _remove_pid()
        logger.info("TaskMind daemon stopped.")
        print("TaskMind daemon stopped.")
