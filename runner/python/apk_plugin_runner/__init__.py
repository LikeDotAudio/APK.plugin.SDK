# Part of the APK.audio project — http://APK.audio — made by Anthony Kuzub
# MIT Licence. Free, for everyone, for ever. Full text in LICENSE at the root.
"""`apk_plugin_runner` — the Python twin of the `apk-plugin-runner` crate.

LOOSE-template-python-twin / LOOSE-bluetooth-no-runtime. A plugin written in
Python does the same five things a Rust plugin's `main!` does, through this
module, so the bus cannot tell which language a plugin was written in:

  1. Broker    `MQTT_HOST` / `MQTT_PORT` (127.0.0.1:1883), `APK_MQTT_USERNAME`
               / `APK_MQTT_PASSWORD` — the names `broker_auth` reads — and a
               reconnect paced by paho's backoff (1 s .. 30 s), never a hot loop.
  2. Config    the plugin's own `config.ini` (or the file `APK_PLUGIN_CONFIG`
               names), published retained to `<topic>/config` as
               `{"value": {section: {key: value}}, "path", "source": "plugin"}`,
               `<topic>/status = online`, and `<topic>/host` on a broker the
               bench owns — `publish_config` in RUNNER/src/lib.rs.
  3. Liveness  the session's LAST WILL is `incoming/state = unavailable`,
               retained; `incoming/telemetry` ticks every 15 s while it lives,
               and the tick rewrites the health file `apk-plugin-health` reads.
  4. Selftest  `outgoing/selftest` (engage) and `outgoing/selftest_run` (a
               depth, never retained) for THIS module, answered through the
               plugin's `self_test(depth_word) -> report JSON` door, with the
               verdict `running` published before the run, and the same
               `the_broker_session_carried_this_press` check appended.
               `outgoing/api_get` republishes `incoming/api` (apk_plugin_api).
  5. Watchdog  every agent runs on its own thread. A thread that returns exits
               70, one that raises exits 101, an agent that has called
               `tick()` and stops calling it for `APK_PLUGIN_STALL_SECS`
               (120) exits 75, the session thread dying exits 69, and
               SIGTERM/SIGINT exit 143 within a second.
  6. Loopback  `main(..., manifest="agent.toml")` serves `GET /status` and
               `GET /telemetry` on 127.0.0.1 at the declared port, the same
               documents as RUNNER/src/loopback.rs, the state from contracts'
               rule; `packet_received()` is what moves `running` to `serving`,
               and every runner publish is counted (ROAD-3478).

The exit codes, the topics, the vocabularies and the report's JSON are the
Rust crate's and contracts/rust/src/self_test.rs's, SPELLED HERE rather than
imported, for the reason apk_plugin_api.py gives: a plugin that builds from
its own repository must not need the monorepo to start, and only SDK tags from
sdk-v0.9 on carry contracts/python beside runner/ (a plugin pinned to an older
tag gets the runner alone). `test_apk_plugin_runner.py`
reads the Rust sources and fails when the two disagree.

    import apk_plugin_runner as runner

    def run_hello_agent(host, port):
        while True:
            ...                       # the work
            runner.tick()             # arms the stall watchdog for this agent

    def self_test(depth_word):
        return runner.run_as_json("hello", depth_word, lambda depth:
            runner.Report("hello", depth).with_(runner.Check.pass_(...)))

    if __name__ == "__main__":
        runner.main(token="hello", config_ini=os.path.join(HERE, "config.ini"),
                    self_test=self_test, run=[run_hello_agent],
                    plugin_crate="apkaudio-hello", plugin_version="0.1.0")
"""

from __future__ import annotations

# The one-file module's namespace, rebuilt name for name (ROAD-3749): plugins and
# tests reach every name, private ones too, as apk_plugin_runner.<name>.
import ipaddress  # noqa: F401
import json  # noqa: F401
import os  # noqa: F401
import re  # noqa: F401
import signal  # noqa: F401
import sys  # noqa: F401
import threading  # noqa: F401
import time  # noqa: F401
from typing import Any, Callable, Dict, Iterable, List, Optional, Tuple  # noqa: F401

from .contract import (  # noqa: F401
    HERE, apk_plugin_api, apk_plugin_panels, ROOT, MODULE_ROOT, LEAF_ENGAGE, LEAF_RUN,
    LEAF_RESULT, LEAF_REPORT, ENGAGEMENTS, VERDICTS, DEPTHS, OUTCOMES, AGENT_STATES, ONLINE_NOW,
    TICK_S, EXIT_AGENT_RETURNED, EXIT_AGENT_PANICKED, EXIT_SESSION_DIED, EXIT_STALLED,
    EXIT_TERMINATED, DEFAULT_STALL_SECS, DEFAULT_HEALTH_FILE, MAX_PACKET, module_incoming,
    module_outgoing, module_inbox, agent_topic, is_online_now
)
from .selftest import (  # noqa: F401
    parse_engaged, parse_depth, covers, Check, Report, run_as_json, agent_state_publications
)
from .report import (  # noqa: F401
    REPORT_CHECK_FAILED, REPORT_JOB_FAILED, REPORT_UNAVAILABLE, REPORT_DOWN, REPORT_EARLY_LINES,
    REPORT_TTL_S, _REPORT, _REPORT_LOCK, _report_contract, report_source, report_install,
    _report_send, level_of, report_log, elog, raise_problem, clear_problem, report_checks,
    report_agent_state, going_down
)
from .settings import (  # noqa: F401
    ini_to_json, module_topic, broker_is_local, broker_address, credentials, health_file,
    stall_after_s, machine_name
)
from .mqtt import (  # noqa: F401
    mqtt_client, connect_failed, agent_client, start_agent_client
)
from .manifest import (  # noqa: F401
    TELEMETRY_PORT_ENV, AGENT_KINDS, IMPLEMENTATION_STATUSES, telemetry_rule, _toml_subset,
    Manifest
)
from .loopback import (  # noqa: F401
    _LOOPBACK, _COUNTERS, _COUNTERS_LOCK, declare, packet_received, published, fatal, counters,
    DISABLED_REASON, module_enabled, _with_disabled, _state, telemetry_document,
    status_document, loopback_api_section, chosen_port, loopback_route, make_loopback_server,
    start_loopback
)
from .ticking import (  # noqa: F401
    _LAST_TICK, _AGENT_TICKS, _SLOT, tick, stalest, stalest_tick, write_health_file,
    answer_arguments, tick_sample
)
from .session import (  # noqa: F401
    Session
)
from .serving import (  # noqa: F401
    _TERMINATE, _AT_STOP, _CONFIG, at_stop, _run_stop_hooks, config, _on_terminate,
    pod_declared, resolve_pod, serve, load_config, main
)
