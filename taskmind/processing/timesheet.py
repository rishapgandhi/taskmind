"""Timesheet generator - groups activities into project time blocks."""
from datetime import date
from collections import defaultdict
from taskmind.database import get_activities_for_date
from taskmind.capture.browser_url import is_browser, get_browser_url


def generate_timesheet(target_date=None):
    """Generate timesheet entries from raw activities for a given date.
    
    Returns list of dicts: {date, project_name, start_time, end_time, duration_minutes, description}
    """
    if target_date is None:
        target_date = date.today().isoformat()

    activities = get_activities_for_date(target_date)
    if not activities:
        return []

    # Group consecutive same-project activities into blocks
    blocks = []
    current_block = None

    for act in activities:
        if act["is_idle"]:
            # Close current block on idle
            if current_block:
                blocks.append(current_block)
                current_block = None
            continue

        project = act["project_name"] or "Unclassified"
        ts = act["timestamp"]

        if current_block and current_block["project"] == project:
            # Extend current block
            current_block["end"] = ts
            current_block["seconds"] += act["duration_seconds"]
            current_block["titles"].append(_activity_desc(act))
        else:
            # Save previous block, start new one
            if current_block:
                blocks.append(current_block)
            current_block = {
                "project": project,
                "start": ts,
                "end": ts,
                "seconds": act["duration_seconds"],
                "titles": [_activity_desc(act)],
            }

    if current_block:
        blocks.append(current_block)

    # Convert to timesheet entries
    entries = []
    for block in blocks:
        duration_min = block["seconds"] // 60

        # Generate description from most common window titles
        desc = _top_titles(block["titles"], limit=3)

        start_time = block["start"][11:16] if len(block["start"]) > 15 else block["start"]
        end_time = block["end"][11:16] if len(block["end"]) > 15 else block["end"]

        entries.append({
            "date": target_date,
            "project_name": block["project"],
            "start_time": start_time,
            "end_time": end_time,
            "duration_minutes": duration_min,
            "description": desc,
        })

    return entries


def _activity_desc(act):
    """Get description for an activity - prefer browser URL over tab title."""
    url = act.get("browser_url") or ""
    if url:
        return url
    # For old data without stored URL, try lookup for browser windows
    if is_browser(act.get("app_name", ""), act.get("window_class", "")):
        url = get_browser_url(act.get("window_title", ""), act.get("app_name", ""), act.get("window_class", ""))
        if url:
            return url
    return act.get("window_title") or ""


def _top_titles(titles, limit=3):
    """Get most frequent window titles as description."""
    counts = defaultdict(int)
    for t in titles:
        if t.strip():
            counts[t] += 1
    sorted_titles = sorted(counts.keys(), key=lambda x: counts[x], reverse=True)
    top = sorted_titles[:limit]
    return "; ".join(top) if top else ""
