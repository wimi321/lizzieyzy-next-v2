"""Semantic checks for recorded protocol data, independent of the Rust parser.

This validates captured JSON messages and their relationships, never source code.
Cancellation ACKs and final results may race; only causally required order is
checked. The observations in each JSONL preserve the actual order from that run.
"""

import json
import math


SCENARIOS = (
    "query-version", "opening-ownership-policy", "during-search",
    "terminate-running", "terminate-queued", "terminate-all",
    "analyze-turns-priority", "board-9", "pass", "illegal-move", "unknown-field",
)


def require(condition, message):
    if not condition:
        raise ValueError(message)


def validate_transcript(rows, scenario):
    require(bool(rows), "empty transcript")
    meta = rows[0]
    require(meta.get("format_version") == 1, "unsupported transcript format")
    require(meta.get("scenario") == scenario, "scenario metadata mismatch")
    for key in ("katago_version", "model", "backend"):
        require(isinstance(meta.get(key), str) and bool(meta[key]), "missing " + key)
    for key in ("model_sha256", "config_sha256"):
        value = meta.get(key, "")
        require(len(value) == 64 and all(c in "0123456789abcdef" for c in value),
                "invalid " + key)
    require(meta.get("analysis_threads") == 1, "queued scenarios require one worker")
    require(meta.get("report_perspective") == "BLACK", "unexpected perspective")
    effective_config = {row.get("line", "").strip() for row in rows[1:]
                        if row.get("dir") == "err"}
    for setting in ("numAnalysisThreads = 1", "reportAnalysisWinratesAs = BLACK",
                    "analysisPVLen = 8"):
        require(setting in effective_config, "effective engine config mismatch: " + setting)
    incoming, outgoing = [], []
    previous_time = -1
    for index, event in enumerate(rows[1:], 1):
        require(event.get("dir") in ("in", "out", "err"), "invalid direction")
        stamp = event.get("t_ms")
        require(isinstance(stamp, (float, int)) and math.isfinite(stamp) and
                stamp >= previous_time, "timestamps must be finite and monotonic")
        previous_time = stamp
        require(isinstance(event.get("line"), str), "line must be raw text")
        if event["dir"] == "err":
            continue
        value = json.loads(event["line"])
        require(isinstance(value, dict), "protocol line is not an object")
        (incoming if event["dir"] == "in" else outgoing).append((index, value))
    requests = {value["id"]: (index, value) for index, value in incoming}
    require(len(requests) == len(incoming), "duplicate input id")
    for index, value in outgoing:
        require(value.get("id") in requests, "response without a request")
        require(index > requests[value["id"]][0], "response precedes its request")

    def outputs(query_id):
        return [(i, value) for i, value in outgoing if value.get("id") == query_id]

    def finals(query_id):
        return [(i, value) for i, value in outputs(query_id)
                if value.get("isDuringSearch") is False]

    def partials(query_id):
        return [(i, value) for i, value in outputs(query_id)
                if value.get("isDuringSearch") is True]

    def final(query_id):
        values = finals(query_id)
        require(len(values) == 1, query_id + ": expected one final")
        return values[0]

    for query_id, (_, request) in requests.items():
        values = outputs(query_id)
        action = request.get("action")
        if action == "query_version":
            require(len(values) == 1 and values[0][1].get("version", "").lstrip("v") ==
                    meta["katago_version"].lstrip("v"), "missing/mismatched version response")
            require("git_hash" in values[0][1], "missing engine git hash")
        elif action in ("terminate", "terminate_all"):
            require(len(values) == 1 and values[0][1] == request, "missing action ACK")
        elif query_id == "illegal":
            require(len(values) == 1 and "error" in values[0][1], "missing illegal-move error")
        else:
            expected_turns = request.get("analyzeTurns", [len(request["moves"])])
            terminal = finals(query_id)
            require(sorted(v["turnNumber"] for _, v in terminal) == sorted(expected_turns),
                    query_id + ": missing or duplicate final turn")
            for index, value in values:
                require("error" not in value, query_id + ": unexpected engine error")
                if "warning" in value:
                    require(scenario == "unknown-field" and query_id == "unknown",
                            "unexpected warning")
                    continue
                require(type(value.get("isDuringSearch")) is bool,
                        "analysis response missing isDuringSearch")
                if value.get("noResults"):
                    require(value["isDuringSearch"] is False and
                            "rootInfo" not in value and "moveInfos" not in value,
                            "invalid noResults final")
                    require(any(requests[query_id][0] < action_index < index and
                                (action.get("action") == "terminate_all" or
                                 (action.get("action") == "terminate" and
                                  action.get("terminateId") == query_id))
                                for action_index, action in incoming),
                            "noResults without a preceding cancellation")
                    continue
                require(value.get("rootInfo", {}).get("visits", 0) > 0,
                        "missing search visits")
                require(0 <= value["rootInfo"].get("winrate", -1) <= 1,
                        "invalid root winrate")
                require(isinstance(value.get("moveInfos"), list) and bool(value["moveInfos"]),
                        "missing moveInfos")
                for move in value["moveInfos"]:
                    # analysisPVLen counts moves AFTER the candidate's first move.
                    require(len(move["pv"]) <= 9,
                            "PV exceeds configured limit: {} moves".format(len(move["pv"])))
                if value["isDuringSearch"]:
                    require(any(i > index and v["turnNumber"] == value["turnNumber"]
                                for i, v in terminal), "partial after final")

    def cancellation(query_id, action_id, queued=False):
        require(action_id in requests, "missing cancel request")
        require(requests[action_id][1]["action"] in ("terminate", "terminate_all"),
                "not a cancellation")
        action = requests[action_id][1]
        require(action["action"] == "terminate_all" or action.get("terminateId") == query_id,
                "cancellation targets another query")
        terminal_index, terminal = final(query_id)
        require(terminal_index > requests[action_id][0], "final preceded cancellation")
        if queued:
            require(terminal.get("noResults") is True and not partials(query_id),
                    "queued cancellation must have noResults and no partial")
        else:
            require(partials(query_id) and partials(query_id)[0][0] < requests[action_id][0],
                    "running cancellation requires an earlier partial")
            require(not terminal.get("noResults") and
                    terminal["rootInfo"]["visits"] < requests[query_id][1]["maxVisits"],
                    "running query completed normally before cancellation")

    if scenario == "query-version":
        require(len(incoming) == 1, "version scenario includes extra queries")
    elif scenario == "opening-ownership-policy":
        result = final("opening")[1]
        require(len(result.get("ownership", [])) == 361, "missing 19x19 ownership")
        require(len(result.get("policy", [])) == 362, "missing 19x19 plus pass policy")
    elif scenario == "during-search":
        require(len(partials("stream")) >= 3, "need at least three partials")
        require(final("stream")[1]["rootInfo"]["visits"] >= 200, "incomplete search")
    elif scenario == "terminate-running":
        cancellation("running", "cancel-running")
    elif scenario == "terminate-queued":
        cancellation("queued", "cancel-queued", queued=True)
        cancellation("blocker", "cancel-blocker")
        require(partials("blocker")[0][0] < requests["queued"][0],
                "queued query was not sent behind an active blocker")
    elif scenario == "terminate-all":
        cancellation("blocker", "cancel-all")
        for query_id in ("queued", "queued-2"):
            cancellation(query_id, "cancel-all", queued=True)
            require(partials("blocker")[0][0] < requests[query_id][0],
                    "queued query was not sent behind an active blocker")
    elif scenario == "analyze-turns-priority":
        request = requests["batch"][1]
        require(request["analyzeTurns"] == [0, 1, 2, 3, 4] and
                request["priorities"] == [0, 40, 30, 20, 10], "missing batch priorities")
        require([v["turnNumber"] for _, v in finals("batch")] == [1, 2, 3, 4, 0],
                "batch did not execute in priority order")
        require(partials("blocker")[0][0] < requests["batch"][0], "batch missing blocker")
        cancellation("blocker", "cancel-blocker")
    elif scenario == "board-9":
        request = requests["small"][1]
        require(request["boardXSize"] == request["boardYSize"] == 9, "not a 9x9 query")
    elif scenario == "pass":
        require(any(move == "pass" for _, move in requests["pass"][1]["moves"]),
                "no pass in the input")
    elif scenario == "illegal-move":
        require(outputs("illegal")[0][1].get("field") == "moves", "wrong error field")
    elif scenario == "unknown-field":
        warnings = [v for _, v in outputs("unknown") if "warning" in v]
        require(len(warnings) == 1 and warnings[0].get("field") == "fixtureUnknownField",
                "missing unknown-field warning")
    elif scenario == "human-sl":
        require(bool(meta.get("human_model")), "missing human model metadata")
        require(len(final("human")[1].get("humanPolicy", [])) == 362,
                "missing HumanSL policy")
    else:
        raise ValueError("unknown scenario: " + scenario)
