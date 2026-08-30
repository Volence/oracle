#!/usr/bin/env python3
"""Tests for tools/legacy_accept_table.py.

Two families, deliberately separated:

FIXTURE tests parse a small synthetic C++ file whose ground truth is fixed by
construction, so exact-value assertions are legitimate there. The fixture's
accessors are given DIFFERENT semantics from the real ControlSocket.cpp -- a
coercion set of {"on","off"} instead of {"true","1","yes"}, `$` as the only hex
prefix, `get` accepting strings only -- precisely so that a tool which had
transcribed the real file's behaviour instead of parsing it would FAIL here.

SOURCE tests run against the real ControlSocket.cpp but assert only properties
re-derived at test time by a different route than the tool used, most
importantly: every line number the table reports must actually contain the key
it claims, checked against the raw file. That check is what catches an
off-by-one in body-offset arithmetic, which a self-consistent parser will
otherwise report with total confidence.

ROW-COMPLETENESS tests (SourceDerivedRowCompleteness) exist because the two
families above could both stay green while the table quietly lost rows. They
re-derive, from the C++ text alone, which method reads which key and which of
those reads has no guard, then demand the table still contain each one as a
well-formed record. The expectation never touches the emitted table: the
table's own output is the thing under test, so an expectation read back out of
it would agree with itself forever.

Every "found nothing" assertion is paired with a POSITIVE CONTROL that plants
the shape and proves the search reports it -- a failing search and an empty
world otherwise produce identical output.

Fixtures that call the tool go through RecordedFixture, so a fixture that
blows up becomes a named failure on each of its tests rather than one
`ERROR: setUpClass` that stops the whole class from running. Measured on the
"drop every unguarded addr row" poison: before, 4 classes collapsed into 4
collection errors and 43 of 48 tests never executed; after, 0 collection
errors and every affected test reports by name.

Run via: tools/run_accept_table_tests.sh
"""

from __future__ import annotations

import inspect
import json
import re
import subprocess
import sys
import tempfile
import traceback
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

import legacy_accept_table as lat  # noqa: E402


# ---------------------------------------------------------------------------
# Synthetic fixture -- ground truth is whatever this text says, by construction.
# ---------------------------------------------------------------------------

FIXTURE = r'''
namespace ControlSocket {

struct JsonObj
{
    const json* p = nullptr;

    bool has(const std::string& k) const
    {
        return p && p->is_object() && p->contains(k) && !p->at(k).is_null();
    }
    std::string get(const std::string& k, const std::string& d = "nothing") const
    {
        if (!has(k)) return d;
        const json& v = p->at(k);
        if (v.is_string()) return v.get<std::string>();
        return d;
    }
    long long getInt(const std::string& k, long long d = 7) const
    {
        if (!has(k)) return d;
        const json& v = p->at(k);
        if (v.is_number_integer()) return v.get<long long>();
        if (v.is_string())
        {
            const std::string s = v.get<std::string>();
            if (s.empty()) return d;
            try
            {
                if (s[0] == '$') return (long long)std::stoll(s.substr(1), nullptr, 16);
                return (long long)std::stoll(s, nullptr, 8);
            }
            catch (...) { return d; }
        }
        return d;
    }
    uint32_t getU32(const std::string& k, uint32_t d = 7) const
    {
        return (uint32_t)getInt(k, (long long)d);
    }
    bool getBool(const std::string& k, bool d = false) const
    {
        if (!has(k)) return d;
        const json& v = p->at(k);
        if (v.is_boolean()) return v.get<bool>();
        if (v.is_string())
        {
            const std::string s = v.get<std::string>();
            return (s == "on" || s == "off");
        }
        return d;
    }
};

static bool ResolveThing(const JsonObj& req, uint32_t& out)
{
    if (req.has("thing"))
    {
        out = req.getU32("thing");
        return true;
    }
    return false;
}

static std::vector<std::string> ParseTags(const JsonObj& req)
{
    std::vector<std::string> out;
    if (req.p && req.p->contains("tags") && (*req.p)["tags"].is_array())
        for (const auto& e : (*req.p)["tags"])
            if (e.is_string()) out.push_back(e.get<std::string>());
    return out;
}

// A raw touch whose key is NOT a string literal -- must surface as unparsed.
static std::string PeekDynamic(const JsonObj& req, const std::string& dynKey)
{
    if (req.p && req.p->contains(dynKey)) return "hit";
    return "miss";
}

static std::string OpAlpha(const JsonObj& req, const Context& ctx)
{
    const std::string name = req.get("name");
    const bool loud = req.getBool("loud", true);
    return Reply(name, loud);
}

static std::string OpBeta(const JsonObj& req, const Context& ctx)
{
    uint32_t a = 0;
    if (!ResolveThing(req, a)) return ErrorReply("need thing");
    if (!req.has("count")) return ErrorReply("missing 'count'");
    const int n = (int)req.getInt("count", 3);
    return Reply(a, n);
}

static std::string OpGamma(const JsonObj& req, const Context& ctx)
{
    std::vector<std::string> tags = ParseTags(req);
    return Reply(tags);
}

static std::string OpDelta(const JsonObj& req, const Context& ctx)
{
    return Reply(PeekDynamic(req, "x"));
}

// The orthogonality cases. Guardedness and default-explicitness are
// independent properties, and each combination appears here exactly once so a
// tool that derives either from the other is caught in both directions.
static std::string OpEpsilon(const JsonObj& req, const Context& ctx)
{
    // explicit default, NO guard -> must read as unguarded
    const uint32_t addr = req.getU32("addr", 99);
    return Reply(addr);
}

static std::string OpZeta(const JsonObj& req, const Context& ctx)
{
    // no explicit default, WITH guard -> must read as guarded
    if (req.has("v"))
    {
        return Reply(req.getInt("v"));
    }
    return ErrorReply("need v");
}

static std::string OpEta(const JsonObj& req, const Context& ctx)
{
    // a has() in an unrelated branch protects nothing outside it
    if (req.has("other"))
    {
        Note("unrelated");
    }
    const int a = req.getInt("a", 5);
    if (req.has("a"))
    {
        Note("too late to matter");
    }
    return Reply(a);
}

static std::string OpTheta(const JsonObj& req, const Context& ctx)
{
    // negated has() that does NOT leave the function guards nothing
    if (!req.has("b"))
    {
        Note("missing b, carrying on anyway");
    }
    const int b = req.getInt("b", 1);
    return Reply(b);
}

static std::string OpIota(const JsonObj& req, const Context& ctx)
{
    // negated has() that DOES leave -> guards everything after it
    if (!req.has("c")) return ErrorReply("missing 'c'");
    const int c = req.getInt("c", 1);
    return Reply(c);
}

static std::string OpKappa(const JsonObj& req, const Context& ctx)
{
    // one key, two sites: one inside a guard block, one outside it. The real
    // file has no such key today, so the fixture carries the case.
    int d = 0;
    if (req.has("dual"))
    {
        d = (int)req.getInt("dual");
    }
    return Reply(d + (int)req.getInt("dual", 4));
}

static std::string OpQuiet(const JsonObj&, const Context& ctx)
{
    return Reply();
}

static const std::unordered_map<std::string, Handler>& Handlers()
{
    static const std::unordered_map<std::string, Handler> h = {
        {"alpha",  OpAlpha},
        {"beta",   OpBeta},
        {"gamma",  OpGamma},
        {"delta",  OpDelta},
        {"epsilon", OpEpsilon},
        {"zeta",   OpZeta},
        {"eta",    OpEta},
        {"theta",  OpTheta},
        {"iota",   OpIota},
        {"kappa",  OpKappa},
        {"quiet",  OpQuiet},
        {"ghost",  OpGhost},
    };
    return h;
}

static std::string LegacyOp(const std::string& canonical)
{
    if (canonical == "beta_full") return "beta";
    return canonical;
}
static std::string CanonicalOp(const std::string& legacy)
{
    if (legacy == "beta") return "beta_full";
    return legacy;
}

static std::vector<std::string> AdvertisedMethods()
{
    std::vector<std::string> out;
    for (const auto& kv : Handlers()) out.push_back("legacy/" + CanonicalOp(kv.first));
    return out;
}

static json RunMethod(const std::string& method, const JsonObj& params, const Context& ctx)
{
    auto it = Handlers().find(method);
    return json();
}

static std::optional<std::string> HandleMessage(const std::string& line, Conn& conn,
                                                const Context& ctx)
{
    json msg = json::parse(line, nullptr, false);
    json params = msg.contains("params") && msg["params"].is_object() ? msg["params"] : json::object();
    JsonObj p; p.p = &params;

    if (method == "greet")
    {
        const int v = params.contains("wire") && params["wire"].is_number_integer()
                      ? params["wire"].get<int>() : 0;
        bool chatty = false;
        if (params.contains("caps") && params["caps"].is_object())
            chatty = params["caps"].value("chatty", false);
        return ok(v, chatty);
    }
    return ok(RunMethod(method, p, ctx));
}

}
'''


def build_fixture(text: str = FIXTURE) -> dict:
    return lat.generate(text, {"repo": None, "file": "<fixture>", "revision": None,
                               "revision_unavailable_reason": "fixture"})


# ---------------------------------------------------------------------------
# Fixtures that call the tool must not be able to take the suite with them
# ---------------------------------------------------------------------------


class RecordedFixture(unittest.TestCase):
    """Base for classes whose shared fixture calls into the tool under test.

    A `setUpClass` that raises collapses its entire class into ONE
    `ERROR: setUpClass (...)` and none of its tests run. That still exits
    non-zero, so a defect is still "caught" -- but it is caught by a crash
    during fixture collection rather than by any assertion that names the
    defect, and it stops being caught the moment somebody reorders, splits or
    re-parents the class. A poison that dropped every unguarded `addr` row was
    caught exactly that way: 43 of 48 tests never executed, and not one of the
    failures that did fire mentioned a missing row.

    Here the exception is RECORDED and re-raised as a named FAILURE on every
    test in the class, so the count of what broke stays the count of what
    broke and each test says, in its own words, what it could not evaluate.
    `SkipTest` is passed through unchanged: an absent reference checkout is a
    skip, not a failure.
    """

    fixture_error: str | None = None

    @classmethod
    def build_shared(cls):  # pragma: no cover - overridden by every subclass
        raise NotImplementedError

    @classmethod
    def setUpClass(cls):
        cls.fixture_error = None
        try:
            cls.build_shared()
        except unittest.SkipTest:
            raise
        except Exception:  # noqa: BLE001 - recorded, never swallowed
            cls.fixture_error = traceback.format_exc()

    def setUp(self):
        if self.fixture_error:
            self.fail(
                f"{type(self).__name__}: the shared fixture could not be built, so "
                f"this test could not be evaluated. Reported as a named FAILURE "
                f"rather than a collection error, and never as a pass:\n"
                f"{self.fixture_error}")


class FixtureAccessorSemantics(RecordedFixture):
    """Accessor facts must come out of the parsed text, not out of the tool."""

    @classmethod
    def build_shared(cls):
        cls.doc = build_fixture()
        cls.acc = cls.doc["accessors"]

    def test_implicit_defaults_come_from_the_signature(self):
        # The fixture's defaults are deliberately NOT the real file's
        # ("" / 0 / 0 / false). A transcribing tool reports the real file's.
        self.assertEqual(self.acc["get"]["implicit_default"], "nothing")
        self.assertEqual(self.acc["getInt"]["implicit_default"], 7)
        self.assertEqual(self.acc["getU32"]["implicit_default"], 7)
        self.assertEqual(self.acc["getBool"]["implicit_default"], False)

    def test_string_coercion_set_comes_from_the_comparisons(self):
        self.assertEqual(self.acc["getBool"]["string_coercion_set"], ["on", "off"])

    def test_accepted_types_come_from_the_predicates(self):
        # The fixture's `get` accepts strings only; the real file's also takes
        # numbers and booleans.
        self.assertEqual(self.acc["get"]["accepted_json_types"], ["string"])
        self.assertEqual(self.acc["getBool"]["accepted_json_types"],
                         ["boolean", "string"])

    def test_numeric_string_forms_come_from_the_parse_code(self):
        self.assertEqual(self.acc["getInt"]["string_numeric_prefixes"], ["$"])
        self.assertEqual(self.acc["getInt"]["string_radices"], [8])

    def test_trailing_garbage_in_a_numeric_string_is_not_rejected(self):
        # Derived from the source fact that stoll's `pos` out-param is nullptr
        # and nothing checks full consumption, so "12abc" reads 12.
        self.assertFalse(self.acc["getInt"]["trailing_garbage_rejected"])
        self.assertIn("stoll", FIXTURE)  # positive control: the call is present

    def test_delegation_is_followed(self):
        self.assertEqual(self.acc["getU32"]["delegates_to"], "getInt")
        self.assertEqual(self.acc["getU32"]["accepted_json_types"],
                         self.acc["getInt"]["accepted_json_types"])
        self.assertEqual(self.acc["getU32"]["string_numeric_prefixes"], ["$"])

    def test_unaccepted_string_does_not_fall_back_to_the_caller_default(self):
        # The branch returns a comparison, not `d`. This is the property that
        # makes a correctly-spelled, guarded key read false anyway.
        self.assertFalse(self.acc["getBool"]["string_branch_falls_back_to_default"])
        self.assertTrue(self.acc["getInt"]["string_branch_falls_back_to_default"])

    def test_has_semantics_are_parsed(self):
        self.assertFalse(self.acc["has"]["checks_type"])
        self.assertTrue(self.acc["has"]["rejects_null"])


class FixtureTableShape(RecordedFixture):
    @classmethod
    def build_shared(cls):
        cls.doc = build_fixture()
        cls.m = cls.doc["methods"]

    def test_namespace_prefix_and_canonical_rename_come_from_source(self):
        # The fixture uses "legacy/" and renames beta -> beta_full.
        self.assertIn("legacy/alpha", self.m)
        self.assertIn("legacy/beta_full", self.m)
        self.assertNotIn("emulator/alpha", self.m)

    def test_every_record_carries_the_four_required_fields(self):
        for method, keys in self.m.items():
            for key, rec in keys.items():
                for field in ("accessor", "default", "guarded_by", "accepted_shapes"):
                    self.assertIn(field, rec, f"{method}.{key} missing {field}")

    def test_unguarded_read_is_distinguished_from_guarded_read(self):
        alpha = self.m["legacy/alpha"]
        self.assertIsNone(alpha["name"]["guarded_by"])
        beta = self.m["legacy/beta_full"]
        self.assertIsNotNone(beta["count"]["guarded_by"])
        self.assertIn('has("count")', beta["count"]["guarded_by"]["expr"])

    def test_guardedness_is_orthogonal_to_default_explicitness(self):
        """The single most important property of the table.

        Deriving guardedness from the presence or absence of a default argument
        produces false positives AND excludes real unguarded sites by
        construction. Both directions are asserted here.
        """
        # explicit default, no guard
        eps = self.m["legacy/epsilon"]["addr"]
        self.assertTrue(eps["default"]["explicit"])
        self.assertIsNone(eps["guarded_by"],
                          "a read with an explicit default is NOT thereby guarded")
        # no explicit default, guarded
        zeta = self.m["legacy/zeta"]["v"]
        self.assertFalse(zeta["default"]["explicit"])
        self.assertIsNotNone(zeta["guarded_by"],
                             "a read without an explicit default is NOT thereby unguarded")

    def test_a_has_in_an_unrelated_branch_does_not_guard(self):
        eta = self.m["legacy/eta"]["a"]
        self.assertIsNone(eta["guarded_by"],
                          "a has() outside the block containing the read guards nothing")
        # positive control: the same fixture DOES contain a has() for that key,
        # so this is a dominance result and not a "no has() found" result.
        self.assertIn('req.has("a")', FIXTURE)

    def test_a_negated_has_that_does_not_leave_the_function_guards_nothing(self):
        self.assertIsNone(self.m["legacy/theta"]["b"]["guarded_by"])
        # positive control: the same shape WITH a return does guard.
        iota = self.m["legacy/iota"]["c"]
        self.assertIsNotNone(iota["guarded_by"])
        self.assertEqual(iota["guarded_by"]["kind"], "early_bail")

    def test_a_key_guarded_on_only_some_sites_summarises_as_unguarded(self):
        """One unguarded path is the path a caller must plan for.

        Summarising a mixed key as guarded would hide exactly that path, so the
        key-level answer is the unguarded one and the mix is flagged.
        """
        dual = self.m["legacy/kappa"]["dual"]
        self.assertEqual(len(dual["read_sites"]), 2)
        self.assertTrue(dual.get("partially_guarded"))
        self.assertIsNone(dual["guarded_by"])
        # ...and the per-site detail still distinguishes the two.
        per_site = [bool(s["guarded_by"]) for s in dual["read_sites"]]
        self.assertEqual(sorted(per_site), [False, True])
        self.assertTrue(dual["unguarded_sites"])

    def test_guards_record_what_they_cover(self):
        # has() is satisfied by any present non-null value, so it covers
        # absence and not type. That distinction is carried explicitly rather
        # than left for a reader to infer.
        g = self.m["legacy/zeta"]["v"]["guarded_by"]
        self.assertEqual(g["guards_against"], "absence")
        self.assertFalse(g["guard_checks_type"])

    def test_unlisted_string_does_not_yield_the_declared_default(self):
        loud = self.m["legacy/alpha"]["loud"]
        self.assertEqual(loud["default"]["value"], True)
        sh = loud["accepted_shapes"]
        self.assertTrue(sh["declared_default_is_not_applied_to_unlisted_strings"])
        self.assertEqual(sh["effective_value_for_unlisted_string"], False)
        self.assertTrue(sh["unlisted_string_inverts_declared_default"])

    def test_explicit_default_beats_the_accessor_default(self):
        self.assertEqual(self.m["legacy/alpha"]["loud"]["default"],
                         {"value": True, "explicit": True, "source": "call_site"})
        self.assertEqual(self.m["legacy/alpha"]["name"]["default"],
                         {"value": "nothing", "explicit": False,
                          "source": "accessor_signature"})

    def test_helper_keys_are_attributed_to_the_calling_method(self):
        # `thing` is read inside ResolveThing, never in OpBeta itself.
        beta = self.m["legacy/beta_full"]
        self.assertIn("thing", beta)
        self.assertEqual(beta["thing"]["read_sites"][0]["in_function"], "ResolveThing")
        self.assertEqual(beta["thing"]["guarded_by"]["in_function"], "ResolveThing")
        # ...and not to a method that never calls the helper.
        self.assertNotIn("thing", self.m["legacy/alpha"])

    def test_raw_json_read_is_recorded_with_its_element_shape(self):
        tags = self.m["legacy/gamma"]["tags"]
        self.assertEqual(tags["accessor"], "raw_json")
        self.assertEqual(tags["accepted_shapes"]["json_types"], ["array"])
        self.assertEqual(tags["accepted_shapes"]["element_json_types"], ["string"])

    def test_method_with_no_parameters_is_present_and_empty(self):
        # Present-and-empty and absent are different claims; a consumer must be
        # able to tell "takes nothing" from "not analysed".
        self.assertIn("legacy/quiet", self.m)
        self.assertEqual(self.m["legacy/quiet"], {})

    def test_pre_dispatch_method_is_discovered(self):
        self.assertIn("greet", self.m)
        self.assertIn("wire", self.m["greet"])
        self.assertEqual(self.m["greet"]["wire"]["accepted_shapes"]["json_types"],
                         ["number(integer)"])

    def test_nested_defaulting_read_is_not_recorded_as_a_guard(self):
        rec = self.m["greet"]["caps.chatty"]
        self.assertIsNone(rec["guarded_by"])
        self.assertEqual(rec["default"]["value"], False)
        self.assertTrue(rec["default"]["explicit"])

    def test_envelope_substitution_is_recorded(self):
        env = self.doc["envelope"]
        self.assertTrue(env["parsed"])
        self.assertEqual(env["envelope_member"], "params")
        self.assertEqual(env["substituted_when_absent_or_wrong_shape"], {})


class FixtureLoudness(RecordedFixture):
    """Unparsable constructs must appear as unparsed, never be omitted."""

    @classmethod
    def build_shared(cls):
        cls.doc = build_fixture()

    def test_missing_handler_body_is_reported_not_dropped(self):
        # `ghost` is in Handlers() with no OpGhost definition anywhere.
        self.assertIn("legacy/ghost", self.doc["methods"])
        self.assertIn("__unparsed__", self.doc["methods"]["legacy/ghost"])
        self.assertTrue(
            any(u["method"] == "legacy/ghost"
                for u in self.doc["coverage"]["unresolved_handlers"]))
        self.assertFalse(self.doc["coverage"]["complete"])

    def test_a_missing_handler_is_not_rendered_as_taking_no_parameters(self):
        ghost = self.doc["methods"]["legacy/ghost"]
        quiet = self.doc["methods"]["legacy/quiet"]
        self.assertNotEqual(ghost, quiet)
        self.assertTrue(ghost["__unparsed__"]["unparsed"])

    def test_raw_read_with_a_non_literal_key_is_reported(self):
        delta = self.doc["methods"]["legacy/delta"]
        unparsed = [k for k in delta if k.startswith("__unparsed_raw_read")]
        self.assertTrue(unparsed, f"expected an unparsed raw read, got {list(delta)}")
        self.assertTrue(delta[unparsed[0]]["unparsed"])

    def test_positive_control_a_clean_fixture_reports_complete(self):
        # Without this, "reports incomplete" could mean the completeness check
        # never returns True at all.
        clean = FIXTURE.replace('        {"ghost",  OpGhost},\n', "")
        clean = clean.replace(
            '''static std::string OpDelta(const JsonObj& req, const Context& ctx)
{
    return Reply(PeekDynamic(req, "x"));
}''',
            '''static std::string OpDelta(const JsonObj& req, const Context& ctx)
{
    return Reply(req.get("plain"));
}''')
        clean = clean.replace(
            '''static std::string PeekDynamic(const JsonObj& req, const std::string& dynKey)
{
    if (req.p && req.p->contains(dynKey)) return "hit";
    return "miss";
}''', "")
        doc = build_fixture(clean)
        self.assertEqual(doc["coverage"]["unresolved_handlers"], [])
        self.assertEqual(doc["coverage"]["unparsed_entries"], [])
        self.assertTrue(doc["crosscheck"]["reconciliation"]["agrees"],
                        doc["crosscheck"]["reconciliation"])
        self.assertTrue(doc["coverage"]["complete"])

    def test_positive_control_an_unknown_member_is_reported_unclaimed(self):
        # Plant a read through a member the accessor axis has never heard of.
        # If axis B could not see it, this test would pass vacuously, so the
        # assertion is on the unclaimed list specifically.
        clean = FIXTURE.replace('        {"ghost",  OpGhost},\n', "")
        planted = clean.replace(
            '    const std::string name = req.get("name");',
            '    const std::string name = req.getWidget("name");')
        doc = build_fixture(planted)
        unclaimed = doc["crosscheck"]["reconciliation"]["unclaimed_accesses"]
        self.assertTrue(any(a["member"] == "getWidget" for a in unclaimed),
                        f"planted member not reported: {unclaimed}")
        self.assertFalse(doc["crosscheck"]["reconciliation"]["agrees"])

    def test_unresolvable_revision_is_reported_as_unavailable_not_as_empty(self):
        """Exercise source_revision() itself, not a hand-built fixture value.

        The first version of this test asserted on the revision it had passed
        IN, so it stayed green when source_revision() was mutated to report a
        missing revision as "" -- rendering "couldn't measure" as a value. The
        mutation sweep caught it; the fix is to call the real function.
        """
        with tempfile.TemporaryDirectory() as td:
            outside = Path(td)
            probe = subprocess.run(["git", "-C", str(outside), "rev-parse", "HEAD"],
                                   capture_output=True, text=True)
            if probe.returncode == 0:
                self.skipTest(f"{outside} is unexpectedly inside a git repo")
            meta = lat.source_revision(outside)

        self.assertIsNone(meta["revision"],
                          "an unavailable revision must be None, never a falsy "
                          "stand-in a consumer could pin against")
        self.assertNotEqual(meta["revision"], "")
        self.assertIn("revision_unavailable_reason", meta)
        self.assertTrue(meta["revision_unavailable_reason"])

        # ...and it must be reported loudly downstream, not rendered as blank.
        doc = lat.generate(FIXTURE, meta)
        self.assertIn("UNAVAILABLE", lat.summarize(doc))


class RealSourceInvariants(RecordedFixture):
    """Checks against the real file, re-derived by a different route."""

    @classmethod
    def build_shared(cls):
        try:
            cls.root = lat.find_oracle_old()
        except FileNotFoundError as exc:
            raise unittest.SkipTest(f"oracle-old not available: {exc}")
        cls.path = cls.root / lat.SOURCE_RELPATH
        cls.raw = cls.path.read_text(encoding="utf-8", errors="surrogateescape")
        cls.lines = cls.raw.splitlines()
        cls.doc = lat.generate(cls.raw, lat.source_revision(cls.root))

    def test_every_reported_line_actually_contains_its_key(self):
        """The check that catches body-offset arithmetic errors.

        A parser computing lines from its own slices is perfectly
        self-consistent while being uniformly wrong. This re-reads the raw file
        and demands the cited line mention the key.
        """
        checked = 0
        for method, keys in self.doc["methods"].items():
            for key, rec in keys.items():
                if key.startswith("__unparsed"):
                    continue
                base = key.split(".")[-1]
                for site in rec["read_sites"]:
                    text = self.lines[site["line"] - 1]
                    self.assertIn(f'"{base}"', text,
                                  f'{method}.{key} cites line {site["line"]} '
                                  f'which does not mention it: {text.strip()!r}')
                    checked += 1
        self.assertGreater(checked, 50, "far too few sites checked to be meaningful")

    def test_every_reported_guard_line_actually_contains_a_guard(self):
        checked = 0
        for method, keys in self.doc["methods"].items():
            for key, rec in keys.items():
                g = rec.get("guarded_by")
                if not g:
                    continue
                text = self.lines[g["line"] - 1]
                base = key.split(".")[-1]
                self.assertIn(f'"{base}"', text,
                              f'{method}.{key} guard cites {g["line"]}: {text.strip()!r}')
                self.assertTrue(
                    re.search(r"\.has\s*\(|contains\s*\(", text),
                    f'{method}.{key} guard line has no guard call: {text.strip()!r}')
                checked += 1
        self.assertGreater(checked, 10)

    def test_every_accessor_named_is_declared_on_the_params_struct(self):
        struct = re.search(r"struct\s+JsonObj\s*\{", self.raw)
        end = lat.match_braces(self.raw, struct.end() - 1)
        declared = set(re.findall(r"\b(\w+)\s*\(\s*const std::string& k",
                                  self.raw[struct.end() - 1:end]))
        declared.add("raw_json")
        for method, keys in self.doc["methods"].items():
            for key, rec in keys.items():
                a = rec.get("accessor")
                for name in ([a] if isinstance(a, str) else (a or [])):
                    self.assertIn(name, declared, f"{method}.{key} -> {name}")

    def test_every_method_name_is_a_literal_in_the_source(self):
        for method, meta in self.doc["method_meta"].items():
            bare = meta["legacy_op"] or method
            self.assertIn(f'"{bare}"', self.raw, method)

    def test_method_set_matches_an_independent_extraction(self):
        """Re-extract the dispatch surface without the tool's parser."""
        hstart = self.raw.index(
            "static const std::unordered_map<std::string, Handler>& Handlers()")
        hbody = self.raw[hstart: self.raw.index("return h;", hstart)]
        ops = set(re.findall(r'\{\s*"(\w+)"\s*,\s*Op\w+\s*\}', hbody))
        # Anchoring on the bare string "Handlers()" matched a COMMENT first and
        # sliced prose, yielding an empty set -- a search that finds nothing
        # looks exactly like a world with nothing in it.
        self.assertGreater(len(ops), 20,
                           "the independent extraction found almost nothing; it is "
                           "measuring its own slice, not the dispatch table")
        predispatch = set(re.findall(r'if \(method == "([^"]+)"\)', self.raw))
        from_meta = {m["legacy_op"] for m in self.doc["method_meta"].values()
                     if m["legacy_op"]}
        self.assertEqual(from_meta, ops)
        self.assertTrue(predispatch.issubset(set(self.doc["methods"])),
                        f"pre-dispatch methods missing: "
                        f"{predispatch - set(self.doc['methods'])}")

    def test_read_site_count_matches_the_reported_total(self):
        seen = set()
        for keys in self.doc["methods"].values():
            for key, rec in keys.items():
                for s in rec["read_sites"]:
                    seen.add((s["in_function"], s["line"], key))
        # Shared helpers are attributed to every caller, so per-method sites
        # over-count; the distinct set is what must match the census.
        self.assertGreaterEqual(len(seen), 1)
        c = self.doc["counts"]
        self.assertEqual(
            c["parameter_read_sites_total"],
            c["accessor_value_read_sites"] + c["raw_read_keys_in_handlers"]
            + c["raw_read_keys_pre_dispatch"])

    def _enclosing_function_body(self, line: int) -> str:
        """The body of the function containing `line`, found without the tool.

        Walks back to the nearest column-0 `static` signature and brace-matches
        forward -- a different route than the parser's function inventory.
        """
        off = sum(len(l) + 1 for l in self.lines[:line - 1])
        starts = [m.start() for m in re.finditer(r"^static\b", self.raw, re.M)
                  if m.start() < off]
        self.assertTrue(starts, f"no enclosing static function for line {line}")
        s = starts[-1]
        brace = self.raw.index("{", self.raw.index(")", s))
        return self.raw[brace: lat.match_braces(self.raw, brace)]

    def test_unguarded_keys_have_no_has_for_that_key_in_their_function(self):
        """Cross-check guardedness by text, not by the tool's dominance logic.

        Guardedness is asserted here WITHOUT reference to whether the read
        carries a default, because those properties are orthogonal and
        filtering one by the other is how both false positives and false
        negatives get in.
        """
        unguarded = guarded = 0
        for method, keys in self.doc["methods"].items():
            for key, rec in keys.items():
                if key.startswith("__unparsed") or rec.get("accessor") == "raw_json":
                    continue
                for site in rec["read_sites"]:
                    body = self._enclosing_function_body(site["line"])
                    present = f'.has("{key}")' in body
                    if site["guarded_by"] is None:
                        # A guard may still exist in an unrelated branch; the
                        # fixture covers dominance. Here: if the tool says
                        # unguarded, no dominating guard may be claimed.
                        unguarded += 1
                    else:
                        g = site["guarded_by"]
                        if g["kind"] != "transitive":
                            self.assertTrue(
                                present,
                                f'{method}.{key} claims a guard at line {g["line"]} '
                                f'but its function contains no has("{key}")')
                        guarded += 1
        self.assertGreater(unguarded, 10)
        self.assertGreater(guarded, 5)

    def test_no_guard_on_the_real_file_covers_type(self):
        """`has()` is satisfied by any present non-null value.

        Re-derived here from the struct text rather than from the tool's
        parsed accessor record.
        """
        struct = re.search(r"struct\s+JsonObj\s*\{", self.raw)
        end = lat.match_braces(self.raw, struct.end() - 1)
        hm = re.search(r"\bhas\s*\([^)]*\)\s*const\s*\{", self.raw[struct.end() - 1:end])
        hbody = self.raw[struct.end() - 1:end]
        hbody = hbody[hm.end() - 1: lat.match_braces(hbody, hm.end() - 1)]
        # The only type test that would make has() type-checking is one applied
        # to the VALUE AT THE KEY. `p->is_object()` tests the params container
        # itself and says nothing about the value, so matching it was measuring
        # the wrong thing.
        value_tests = re.findall(r"at\s*\(\s*k\s*\)\s*\.\s*(is_\w+)\s*\(", hbody)
        self.assertTrue(value_tests, f"has() no longer inspects at(k): {hbody!r}")
        self.assertEqual([t for t in value_tests if t != "is_null"], [],
                         f"has() now applies a type test to the value: {hbody!r}")
        for method, keys in self.doc["methods"].items():
            for key, rec in keys.items():
                g = rec.get("guarded_by")
                if g and rec.get("accessor") != "raw_json":
                    self.assertEqual(g["guards_against"], "absence", f"{method}.{key}")

    def test_hazard_view_does_not_filter_guardedness_by_default_explicitness(self):
        """Positive control against the exact conflation this tool once had.

        The summary view previously listed only unguarded reads with an
        IMPLICIT default, which silently dropped every unguarded read that
        happened to carry one. Both partitions must reconstitute the whole.
        """
        h = self.doc["hazards"]
        whole = {(u["method"], u["key"]) for u in h["unguarded_reads"]}
        impl = {(u["method"], u["key"])
                for u in h["unguarded_reads_with_an_implicit_default"]}
        expl = {(u["method"], u["key"])
                for u in h["unguarded_reads_with_an_explicit_default"]}
        self.assertEqual(impl | expl, whole)
        self.assertEqual(impl & expl, set())
        # Non-vacuous: unguarded reads WITH an explicit default must exist,
        # or this control proves nothing.
        self.assertTrue(expl, "no unguarded read carries an explicit default; "
                              "this control cannot detect the conflation")

    def test_the_two_enumerations_agree_on_the_real_file(self):
        r = self.doc["crosscheck"]["reconciliation"]
        self.assertEqual(r["value_read_delta"], 0, r)
        self.assertEqual(r["guard_delta"], 0, r)
        self.assertEqual(r["unclaimed_accesses"], [], r)
        self.assertEqual(r["identifiers_not_measured"], [], r)

    def test_parse_of_the_real_file_is_complete(self):
        self.assertEqual(self.doc["coverage"]["unparsed_entries"], [])
        self.assertEqual(self.doc["coverage"]["unresolved_handlers"], [])
        self.assertTrue(self.doc["coverage"]["complete"])

    def test_the_table_records_the_revision_it_derived_from(self):
        rev = self.doc["source"]["revision"]
        self.assertIsNotNone(rev, "a table with no revision cannot be pinned")
        self.assertRegex(rev, r"^[0-9a-f]{40}$")
        actual = subprocess.run(["git", "-C", str(self.root), "rev-parse", "HEAD"],
                                capture_output=True, text=True, check=True).stdout.strip()
        self.assertEqual(rev, actual)

    def test_comments_are_not_mistaken_for_code(self):
        """Positive control: the file DOES contain `req.path` inside a comment."""
        self.assertTrue(any("req.path" in ln and ln.lstrip().startswith("//")
                            for ln in self.lines),
                        "the comment-shaped decoy is gone; this control is vacuous")
        for keys in self.doc["methods"].values():
            for rec in keys.values():
                for s in rec["read_sites"]:
                    self.assertFalse(self.lines[s["line"] - 1].lstrip().startswith("//"),
                                     f"read site {s} points at a comment line")


# ---------------------------------------------------------------------------
# Row completeness -- the expectation is re-derived from the C++, per test
# ---------------------------------------------------------------------------
#
# `direct_reads_from_source` and `missing_rows` used to live here. They now
# live in the tool, because `--fail-on-gap` asserts row presence with them
# (ledger L-09) and a second copy here would be free to drift away from the
# one the gate actually runs. They are imported, not re-implemented: these
# tests must exercise the SHIPPED derivation, not a look-alike.

def _names_reachable_from(fn) -> set[str]:
    """Every global/attribute name the compiled function can reach.

    A text search of the source would be fooled by the function's own prose:
    the docstring names `match_braces` to explain why it must not call it. The
    bytecode cannot be fooled that way, and it follows comprehensions and
    nested functions, which a one-line grep would miss.
    """
    names: set[str] = set()
    stack = [fn.__code__]
    while stack:
        code = stack.pop()
        names.update(code.co_names)
        stack.extend(k for k in code.co_consts if hasattr(k, "co_names"))
    return names


direct_reads_from_source = lat.direct_reads_from_source
missing_rows = lat.missing_rows
REQUIRED_RECORD_FIELDS = lat.REQUIRED_RECORD_FIELDS
VALUE_ACCESSOR_RE = lat.VALUE_ACCESSOR_RE


class SourceDerivedRowCompleteness(RecordedFixture):
    """No row may vanish from the table without a named test saying so.

    The expectation is rebuilt from `ControlSocket.cpp` on every test by
    `direct_reads_from_source`, whose independence from the table builder is
    exact and bounded -- see its docstring, and the L-10 tests at the foot of
    this class that hold it. "Shares no code with the table builder" stood
    here once and was false; it shares `blank_comments` and nothing else.
    The document under test is built INSIDE the test path, not in a shared
    fixture that can take the class down with it: if `generate()` raises, that
    becomes a named failure here that still names the rows it could not find.
    """

    @classmethod
    def build_shared(cls):
        try:
            cls.root = lat.find_oracle_old()
        except FileNotFoundError as exc:
            raise unittest.SkipTest(f"oracle-old not available: {exc}")
        cls.raw = (cls.root / lat.SOURCE_RELPATH).read_text(
            encoding="utf-8", errors="surrogateescape")
        # The expectation itself must survive a broken tool, so it is derived
        # here and kept even when the document build below blows up.
        cls.expected = direct_reads_from_source(cls.raw)
        try:
            cls.doc = lat.generate(cls.raw, lat.source_revision(cls.root))
            cls.build_error = None
        except Exception:  # noqa: BLE001 - surfaced per-test, never swallowed
            cls.doc = None
            cls.build_error = traceback.format_exc()

    def _methods(self, what: str) -> dict:
        """The table's methods, or a named failure that says what is unproven."""
        if self.doc is None:
            self.fail(
                f"the accept-table could not be built at all, so {what} could not "
                f"be shown present. An unbuildable table is reported here as a "
                f"FAILURE naming what is unverified -- never as an empty set, a "
                f"zero, or a pass. Build error:\n{self.build_error}")
        return self.doc["methods"]

    # -- controls on the derivation itself ---------------------------------

    def test_the_independent_derivation_is_not_measuring_its_own_slice(self):
        e = self.expected
        self.assertGreater(e["handlers"], 20,
                           "handler extraction found almost nothing")
        self.assertGreater(len(e["pairs"]), 20,
                           "the source scan found almost no parameter reads; a "
                           "search that finds nothing looks exactly like a file "
                           "with nothing in it")
        self.assertTrue(e["unguarded"], "the scan classified NOTHING as unguarded")
        self.assertTrue(e["guarded"],
                        "the scan classified NOTHING as guarded, so its "
                        "guardedness test is not discriminating and its "
                        "'unguarded' list means nothing")

    def test_the_derivation_separates_a_guarded_addr_from_an_unguarded_one(self):
        """The specific discrimination the headline assertion rests on.

        If this scan called every `addr` unguarded it would still 'pass' the
        completeness check below while proving nothing about guardedness.
        """
        unguarded = {m for (m, k) in self.expected["unguarded"] if k == "addr"}
        guarded = {m for (m, k) in self.expected["guarded"] if k == "addr"}
        self.assertTrue(unguarded, "no unguarded addr read found in the source")
        self.assertTrue(guarded,
                        "no has()-guarded addr read found in the source, so this "
                        "scan cannot be shown to tell the two apart")
        self.assertEqual(unguarded & guarded, set())

    # -- the headline ------------------------------------------------------

    def test_unguarded_addr_rows_are_present_and_well_formed(self):
        """The memory-path rows are the ones whose absence is dangerous.

        If these rows go missing the table reads as "these commands take no
        address", i.e. "these commands are safe" -- the exact wrong answer the
        table exists to prevent. Asserted by name, per row, from the source.
        """
        expected = {k: v for k, v in self.expected["unguarded"].items()
                    if k[1] == "addr"}
        self.assertTrue(expected, "vacuous: no unguarded addr read derived")
        methods = self._methods(
            f"the unguarded addr rows {sorted(m for m, _ in expected)}")
        gaps = missing_rows(methods, expected)
        self.assertEqual(gaps, [], "unguarded addr row(s) missing from the "
                                   "table:\n  " + "\n  ".join(gaps))
        for (method, key) in sorted(expected):
            self.assertIsNone(
                methods[method][key]["guarded_by"],
                f"{method}.{key} is unguarded in the source but the table claims "
                f"a guard; a memory-path read reported as guarded is the same "
                f"wrong answer as one reported missing")

    def test_unguarded_addr_rows_reach_the_hazard_view(self):
        """A row can survive in `methods` and still fall out of the summary."""
        expected = {k for k in self.expected["unguarded"] if k[1] == "addr"}
        self.assertTrue(expected, "vacuous: no unguarded addr read derived")
        self._methods(f"the hazard-view entries for {sorted(expected)}")
        seen = {(u["method"], u["key"])
                for u in self.doc["hazards"]["unguarded_reads"]}
        self.assertEqual(
            expected - seen, set(),
            f"unguarded addr read(s) absent from hazards.unguarded_reads: "
            f"{sorted(expected - seen)}. The table would report them as safe.")

    def test_every_key_a_handler_reads_directly_has_a_row(self):
        """The general form: no row of any key may silently disappear."""
        expected = self.expected["pairs"]
        self.assertGreater(len(expected), 20, "vacuous expectation")
        methods = self._methods(f"{len(expected)} source-derived rows")
        gaps = missing_rows(methods, expected)
        self.assertEqual(gaps, [],
                         f"{len(gaps)} of {len(expected)} source-derived rows are "
                         f"missing or hollow:\n  " + "\n  ".join(gaps))

    # -- positive controls: the checker must actually see a removed row ----
    #
    # These build their OWN complete baseline out of the source-derived
    # expectation, rather than maiming the tool's live table. A control that
    # starts from the artefact under test stops being able to run at exactly
    # the moment the artefact breaks -- which is the moment a control is for.
    # Built this way, all four stay GREEN under a poisoned tool and keep
    # proving the checker can still see a removed row.

    def _synthetic_baseline(self) -> tuple[dict, tuple]:
        expected = {k: v for k, v in self.expected["unguarded"].items()
                    if k[1] == "addr"}
        self.assertTrue(expected, "vacuous: no unguarded addr read derived")
        methods: dict = {}
        for (method, key), lines in expected.items():
            methods.setdefault(method, {})[key] = {
                "accessor": "getU32", "default": {"value": 0, "explicit": False},
                "guarded_by": None, "accepted_shapes": {},
                "read_sites": [{"line": ln} for ln in lines],
            }
        self.assertEqual(missing_rows(methods, expected), [],
                         "the synthetic baseline is not itself complete, so "
                         "nothing it detects afterwards means anything")
        return methods, sorted(expected)[0]

    def test_positive_control_a_dropped_row_is_reported(self):
        methods, victim = self._synthetic_baseline()
        del methods[victim[0]][victim[1]]
        gaps = missing_rows(methods, {victim: [1]})
        self.assertTrue(any("row DROPPED" in g and victim[0] in g for g in gaps),
                        f"deleting {victim} was not reported: {gaps}")

    def test_positive_control_a_hollow_row_is_reported(self):
        """The shape the real poison took: the key stays, the record is None."""
        methods, victim = self._synthetic_baseline()
        methods[victim[0]][victim[1]] = None
        gaps = missing_rows(methods, {victim: [1]})
        self.assertTrue(any("HOLLOW" in g and victim[0] in g for g in gaps),
                        f"hollowing {victim} was not reported: {gaps}")

    def test_positive_control_a_relocated_read_site_is_reported(self):
        methods, victim = self._synthetic_baseline()
        rec = methods[victim[0]][victim[1]]
        lines = self.expected["unguarded"][victim]
        rec["read_sites"] = [{"line": ln + 1} for ln in lines]
        gaps = missing_rows(methods, {victim: lines})
        self.assertTrue(any("does not cite the read site" in g for g in gaps),
                        f"shifting {victim}'s read sites was not reported: {gaps}")

    def test_positive_control_a_missing_method_is_reported(self):
        methods, victim = self._synthetic_baseline()
        del methods[victim[0]]
        gaps = missing_rows(methods, {victim: [1]})
        self.assertTrue(any("the method itself is absent" in g for g in gaps),
                        f"deleting method {victim[0]} was not reported: {gaps}")

    # -- the gate `--fail-on-gap` runs (ledger L-09) ------------------------

    def test_row_presence_sees_the_drop_the_cross_check_calls_agreement(self):
        """The whole reason the gate exists, asserted against both halves.

        Dropping the unguarded `addr` rows leaves `coverage.complete` TRUE --
        that is the blindness. The row-presence report must name all four.
        """
        self._methods("the unguarded addr rows")
        victims = sorted(k for k in self.expected["unguarded"] if k[1] == "addr")
        self.assertTrue(victims, "vacuous: no unguarded addr read derived")
        maimed = {m: dict(keys) for m, keys in self.doc["methods"].items()}
        for method, key in victims:
            del maimed[method][key]
        doc = dict(self.doc, methods=maimed)
        self.assertTrue(doc["coverage"]["complete"],
                        "control is void: coverage already reports incomplete, so "
                        "this proves nothing about the gap the drop leaves")
        report = lat.row_presence_report(self.raw, doc)
        self.assertIsNone(report["unmeasurable"])
        for method, key in victims:
            self.assertTrue(
                any(g.startswith(f"{method}.{key}:") for g in report["missing"]),
                f"{method}.{key} was dropped and the report did not name it: "
                f"{report['missing']}")

    def test_row_presence_allows_rows_the_second_derivation_never_claimed(self):
        """The subset direction, on the real table.

        The second derivation is deliberately cruder, so the table legitimately
        holds rows it cannot see. If the assertion were equality this would
        fail forever on a correct table.
        """
        methods = self._methods("the whole table")
        table_rows = sum(len(v) for v in methods.values())
        self.assertGreater(table_rows, len(self.expected["pairs"]),
                           "vacuous: the table holds no row beyond the ones the "
                           "second derivation claims, so subset and equality "
                           "cannot be told apart here")
        report = lat.row_presence_report(self.raw, self.doc)
        self.assertEqual(report["missing"], [],
                         f"the untouched table reports missing rows: "
                         f"{report['missing']}")

    def test_row_presence_that_cannot_be_derived_is_not_reported_as_clean(self):
        """"Nothing missing" and "nothing checked" must not look alike."""
        report = lat.row_presence_report("int main() { return 0; }",
                                         {"methods": {}})
        self.assertIsNotNone(
            report["unmeasurable"],
            "a source the second derivation cannot read produced a clean report; "
            "an unbuildable expectation would then pass the gate silently")
        self.assertEqual(report["checked"], 0)

    # -- L-10: the second derivation must not share the builder's lexer -----
    #
    # L-09 shipped a row-presence gate whose expectation came from a second
    # reading, and its own falsifier proved the two readings were yoked: both
    # called `match_braces`, which skips `"..."` and has no case for `'...'`,
    # so ONE legal character literal truncated the same body for both. The
    # row left the table, the expectation stopped asking for it, and every
    # check printed clean. These tests hold the fix: the second derivation
    # bounds bodies by column-0 signatures and must survive that edit alone.

    def _scratch(self, old: str, new: str) -> str:
        """The real source with one legal edit, in memory. Never written back.

        The perturbation anchor is asserted present: an edit that silently
        matched nothing would leave these tests exercising a pristine file
        and passing for that reason.
        """
        self.assertIn(old, self.raw,
                      f"perturbation anchor {old!r} is not in the source; this "
                      f"test would otherwise 'pass' against an unedited file")
        return self.raw.replace(old, new, 1)

    #: `const char c = '}';` planted at the top of `OpZ80Read`. Legal C++,
    #: and invisible to a brace matcher that does not know char literals.
    TRUNCATOR = ("static std::string OpZ80Read(const JsonObj& req, "
                 "const Context& ctx)\n{\n")

    def test_a_char_literal_brace_does_not_blind_the_second_derivation(self):
        """L-10 acceptance. The builder truncates; the expectation must not.

        The control comes first and is not decoration: unless the BUILDER
        actually drops the row under this edit, the assertion below would be
        satisfied by a source nothing had happened to.
        """
        poisoned = self._scratch(self.TRUNCATOR,
                                 self.TRUNCATOR + "    const char c = '}'; (void)c;\n")
        table = lat.build_table(poisoned)
        self.assertNotIn(
            "addr", table["methods"].get("emulator/z80_read", {}),
            "control is void: the builder did NOT lose emulator/z80_read.addr "
            "under the char-literal edit, so this proves nothing about whether "
            "the second derivation is independent of it")

        expected = direct_reads_from_source(poisoned)
        self.assertIn(
            ("emulator/z80_read", "addr"), expected["pairs"],
            "the second derivation lost emulator/z80_read.addr to the SAME "
            "character literal that truncated the builder -- the two readings "
            "are still yoked to one brace matcher (ledger L-10)")

        report = lat.row_presence_report(poisoned, {"methods": table["methods"]})
        self.assertIsNone(report["unmeasurable"], report["unmeasurable"])
        self.assertTrue(
            any(g.startswith("emulator/z80_read.addr:") for g in report["missing"]),
            f"the truncated table was not reported as missing the row: "
            f"{report['missing']}")

    def test_a_handler_defined_off_column_zero_is_unmeasurable_not_silent(self):
        """The `wrong if` of L-10, asserted in code rather than by hand.

        The boundary walk assumes every handler definition starts at column 0.
        Indent one -- still legal C++ -- and the walk would mis-bound a body.
        The derivation must refuse to run rather than quietly report fewer
        rows, because a shrunken expectation is indistinguishable from a
        complete table.
        """
        sig = "static std::string OpZ80Read(const JsonObj&"
        moved = self._scratch("\n" + sig, "\n    " + sig)
        with self.assertRaises(AssertionError) as caught:
            direct_reads_from_source(moved)
        self.assertIn("column 0", str(caught.exception).lower())
        report = lat.row_presence_report(moved, {"methods": {}})
        self.assertIsNotNone(
            report["unmeasurable"],
            "an indented handler left the derivation reporting a clean, smaller "
            "expectation; the boundary walk's one assumption was violated in "
            "silence")

    def test_an_op_function_the_dispatch_table_never_names_is_unmeasurable(self):
        """The census must agree with the handler count it already validates.

        A column-0 `Op*` definition that `Handlers()` does not dispatch means
        the census and the dispatch table disagree about what a handler is.
        That is the same class of drift as an indented one and gets the same
        answer: fail loudly, do not grade a table against a census that no
        longer describes the file.
        """
        extra = ("static std::string OpNotDispatched(const JsonObj& req)\n"
                 "{\n    return req.get(\"nowhere\");\n}\n\n")
        added = self._scratch("static std::string OpZ80Read",
                              extra + "static std::string OpZ80Read")
        with self.assertRaises(AssertionError) as caught:
            direct_reads_from_source(added)
        self.assertIn("OpNotDispatched", str(caught.exception))

    def test_the_second_derivation_never_calls_the_builders_brace_matcher(self):
        """Structural, because behaviour alone cannot prove a negative.

        The perturbation above shows the derivation survives ONE known lexer
        defect. This shows it cannot inherit the NEXT one: the shared
        body-extent helper is not on its code path at all. That distinction
        is the entire ruling -- an alarm for a known bug is not independence.
        """
        names = _names_reachable_from(lat.direct_reads_from_source)
        self.assertIn("blank_comments", names,
                      "vacuous: this reads no names at all from the function, so "
                      "it would report every helper absent")
        for banned in ("match_braces", "match_parens"):
            self.assertFalse(
                banned in names,
                f"the second derivation calls {banned}, the builder's own "
                f"body-extent helper; a defect in it blinds both readings at "
                f"once, which is exactly what L-10 removed")

    def test_the_body_extent_helper_itself_counts_no_braces(self):
        """The derivation delegates body extent, so the delegate is in scope.

        Moving the brace matching one call deeper would satisfy the test above
        while changing nothing, so the helper it delegates to is checked too.
        """
        names = _names_reachable_from(lat._col0_body)
        for banned in ("match_braces", "match_parens"):
            self.assertFalse(banned in names,
                             f"_col0_body reaches {banned}; the body-extent rule "
                             f"is still the builder's")
        body = inspect.getsource(lat._col0_body).split('"""')[-1]
        self.assertNotIn("{", body,
                         "_col0_body's code mentions a brace; it is supposed to "
                         "bound bodies without looking at one")


class CommandLineInterface(unittest.TestCase):
    TOOL = Path(__file__).resolve().parent / "legacy_accept_table.py"

    def _run(self, *args):
        return subprocess.run([sys.executable, str(self.TOOL), *args],
                              capture_output=True, text=True)

    def test_json_output_parses_and_carries_the_required_shape(self):
        r = self._run("--format", "json")
        self.assertEqual(r.returncode, 0, r.stderr)
        doc = json.loads(r.stdout)
        self.assertEqual(doc["schema"], "oracle/legacy-accept-table/v1")
        self.assertIn("methods", doc)
        self.assertIn("revision", doc["source"])

    def test_fail_on_gap_exits_nonzero_for_an_incomplete_parse(self):
        with tempfile.TemporaryDirectory() as td:
            f = Path(td) / "broken.cpp"
            f.write_text(FIXTURE)  # contains the OpGhost hole
            r = self._run("--source-file", str(f), "--fail-on-gap")
            self.assertEqual(r.returncode, 1, r.stdout[:400])
            self.assertIn("INCOMPLETE", r.stderr)

    def test_positive_control_fail_on_gap_exits_zero_when_clean(self):
        r = self._run("--fail-on-gap", "--format", "summary")
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertIn("AGREES", r.stdout)

    # The gate is only worth anything end to end: the library check can be
    # right while nothing calls it. This runs the real CLI over a copy of the
    # tool with the exact L-09 defect planted -- rows dropped CLEANLY, not a
    # crash, since a sabotage that makes the tool explode proves nothing.
    DROP_ANCHOR = """            entry[key] = _record(
                key,
                names[0] if len(names) == 1 else names,"""
    DROP_POISON = """            if key == "addr" and key_guard is None:
                continue  # planted by the test: drop the unguarded addr rows
"""

    def _poisoned_tool(self, td: str) -> Path:
        src = self.TOOL.read_text(encoding="utf-8")
        self.assertEqual(
            src.count(self.DROP_ANCHOR), 1,
            "the row emitter this control patches has moved; the control is "
            "not planting the defect it claims to plant")
        dst = Path(td) / "legacy_accept_table.py"
        dst.write_text(src.replace(self.DROP_ANCHOR,
                                   self.DROP_POISON + self.DROP_ANCHOR, 1),
                       encoding="utf-8")
        return dst

    def test_fail_on_gap_fails_and_names_rows_dropped_after_the_cross_check(self):
        try:
            root = lat.find_oracle_old()
        except FileNotFoundError as exc:
            raise unittest.SkipTest(f"oracle-old not available: {exc}")
        with tempfile.TemporaryDirectory() as td:
            tool = self._poisoned_tool(td)
            r = subprocess.run(
                [sys.executable, str(tool), "--fail-on-gap", "--format", "summary",
                 "--source", str(root)], capture_output=True, text=True)
        # Red for the RIGHT reason: the old gate's two signals still read clean,
        # which is precisely why they could not catch this.
        self.assertIn("cross-check     : AGREES", r.stdout)
        self.assertIn("parse complete  : yes", r.stdout)
        self.assertEqual(r.returncode, 1,
                         f"the planted row drop did not fail the gate.\n"
                         f"stderr:\n{r.stderr}")
        for method in ("emulator/read_vram", "emulator/write_vram",
                       "emulator/z80_read", "emulator/z80_write"):
            self.assertIn(f"row missing from table: {method}.addr", r.stderr,
                          f"{method}.addr was dropped and the gate did not name "
                          f"it. A count or a collection error is not enough.\n"
                          f"stderr:\n{r.stderr}")

    def test_unresolvable_source_is_an_error_not_an_empty_table(self):
        r = self._run("--source", "/nonexistent/oracle-old-xyz")
        self.assertEqual(r.returncode, 2)
        self.assertIn("does not contain", r.stderr)
        self.assertIn("/nonexistent/oracle-old-xyz", r.stderr)
        # No table at all: a partial or empty table on stdout would be read as
        # "this server accepts nothing", which is a different claim entirely.
        self.assertEqual(r.stdout, "")

    def test_summary_names_the_revision(self):
        r = self._run("--format", "summary")
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertIn("source revision", r.stdout)
        self.assertNotIn("source revision : UNAVAILABLE", r.stdout)


if __name__ == "__main__":
    unittest.main(verbosity=2)
