# Part of the APK.audio project — http://APK.audio — made by Anthony Kuzub
# MIT Licence. Free, for everyone, for ever. Full text in LICENSE at the root.
"""The capture slots, read from topic_grammar.json — the same rules as capture-rule.ts."""
import json

import pytest

from apkaudio_contracts import capture_rule, families


def test_every_kind_is_a_slot():
    assert sorted(capture_rule.CAPTURE_SLOTS) == sorted(capture_rule.CAPTURE_KINDS)


def test_a_kind_outside_the_set_is_refused():
    with pytest.raises(families.TopicError):
        families.topic('capture', captureKind='Screenshot', capture='x')


def test_publish_slot_checks_then_publishes_retained():
    sent = []
    doc = {'device': 'visa/LCR/4263A/Dev711', 'ts': 1.0, 'winners': [':FETC?'], 'losers': []}
    topic = capture_rule.publish_slot(lambda t, p, r: sent.append((t, p, r)), 'Qualified', doc)
    assert topic == 'APK.audio/System/Captures/Qualified/visa-LCR-4263A-Dev711'
    assert sent == [(topic, json.dumps(doc, separators=(',', ':')), True)]
    with pytest.raises(capture_rule.SlotError):
        capture_rule.publish_slot(lambda *a: sent.append(a), 'Qualified', {'device': 'x', 'ts': 1.0})
    assert len(sent) == 1


def test_filed_under_another_device_is_a_problem():
    doc = {'device': 'visa/DMM/34401A/DevttyUSB0', 'ts': 1.0}
    assert capture_rule.capture_problems('Setup', doc, 'visa-DMM-34401A-DevttyUSB0') == []
    assert capture_rule.capture_problems('Setup', doc, 'visa-DMM-34401A-DevttyUSB1')
