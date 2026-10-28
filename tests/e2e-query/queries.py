"""Exercise generated query APIs through PostgreSQL, auth and tenant boundaries."""

import csv
import http.cookiejar
import io
import json
import os
import subprocess
import sys
import urllib.error
import urllib.parse
import urllib.request
import uuid


class Client:
    def __init__(self, base):
        self.base = base
        self.cookies = http.cookiejar.CookieJar()
        self.http = urllib.request.build_opener(urllib.request.HTTPCookieProcessor(self.cookies))
        self.tenant = None

    def request(self, path, params=None, data=None, status=200):
        headers = {}
        if self.tenant:
            headers["X-AppStruct-Tenant"] = self.tenant
        body = None
        if data is not None:
            if path.endswith("/_import.csv"):
                headers["Idempotency-Key"] = str(uuid.uuid4())
            body = data.encode() if isinstance(data, str) else json.dumps(data).encode()
            headers["Content-Type"] = "text/csv" if isinstance(data, str) else "application/json"
            for cookie in self.cookies:
                if cookie.name == "appstruct_csrf":
                    headers["X-CSRF-Token"] = cookie.value
        if params:
            params = {key: json.dumps(value) if key == "where" else value for key, value in params.items()}
            path += "?" + urllib.parse.urlencode(params)
        request = urllib.request.Request(self.base + path, body, headers)
        try:
            response = self.http.open(request, timeout=30)
        except urllib.error.HTTPError as error:
            response = error
        with response:
            raw = response.read()
            assert response.status == status, (path, response.status, raw.decode())
            return raw.decode() if path.split("?")[0].endswith("_export.csv") and status == 200 else json.loads(raw)


client = Client(sys.argv[1])
client.request("/api/auth/register", data={"email": "query@example.test", "password": "query-test-password-2026"})
alpha = client.request("/api/tenant/organizations", status=201, data={"name": "Query Alpha"})["id"]
beta = client.request("/api/tenant/organizations", status=201, data={"name": "Query Beta"})["id"]
client.tenant = alpha
scores = [None, None, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7]
records = []
for index, score in enumerate(scores):
    record = client.request("/api/entries/", status=201, data={
        "title": 'Literal %_ and "quotes",\n行' + "x" * 2000 if index == 0 else f"Row {index:02}",
        "status": ["todo", "active", "done"][index % 3],
        "score": score,
        "amount": str(9007199254740993 + index),
        "payload": {"group": index % 2} if index < 14 else None,
        "secret": "member cannot read this",
    })
    assert "secret" not in record
    records.append(record)

page = client.request("/api/entries/", {"page": 2, "page_size": 4, "sort": "score,-title"})
assert page["meta"] == {"page": 2, "page_size": 4, "total": 15}
assert len(page["data"]) == 4 and all("secret" not in row for row in page["data"])

for sort in ["score,-title", "-score,title", "created_at,-title", "-amount,title"]:
    query = {"sort": sort, "fields": "id,title,amount"}
    expected = client.request("/api/entries/", {**query, "page_size": 100})["data"]
    first = client.request("/api/entries/", {**query, "limit": 3})
    assert "total" not in first["meta"]
    assert all(set(row) == {"id", "title", "amount"} for row in first["data"])
    pages = [first]
    while pages[-1]["meta"]["next_cursor"]:
        assert len(pages) <= 5
        pages.append(client.request("/api/entries/", {**query, "limit": 3, "cursor": pages[-1]["meta"]["next_cursor"]}))
    assert [row for result in pages for row in result["data"]] == expected
    for index in range(len(pages) - 1, 0, -1):
        previous = client.request("/api/entries/", {**query, "limit": 3, "cursor": pages[index]["meta"]["previous_cursor"], "direction": "previous"})
        assert previous["data"] == pages[index - 1]["data"]
    token = first["meta"]["next_cursor"]
    tampered = token[:20] + ("A" if token[20] != "A" else "B") + token[21:]
    client.request("/api/entries/", {**query, "limit": 3, "cursor": tampered}, status=400)
    client.request("/api/entries/", {"sort": "title", "limit": 3, "cursor": token}, status=400)

long_cursor = client.request("/api/entries/", {"sort": "title", "limit": 1, "fields": "id"})["meta"]["next_cursor"]
assert len(long_cursor) > 2000
assert client.request("/api/entries/", {"sort": "title", "limit": 1, "fields": "id", "cursor": long_cursor})["data"][0]["id"] != records[0]["id"]

expression = {"any": [
    {"field": "score", "op": "is_null"},
    {"all": [{"field": "score", "op": "gte", "values": ["5"]}, {"field": "status", "op": "ne", "values": ["done"]}]},
]}
selected = client.request("/api/entries/", {"where": expression, "page_size": 100})
expected_ids = {row["id"] for row in records if row["score"] is None or (row["score"] >= 5 and row["status"] != "done")}
assert {row["id"] for row in selected["data"]} == expected_ids
aggregate = client.request("/api/entries/_aggregate", {"where": expression, "metrics": "count"})
assert aggregate["data"][0]["count"] == len(expected_ids)
assert client.request("/api/entries/", {"where": {"field": "status", "op": "in", "values": ["todo", "active"]}})["meta"]["total"] == 10
assert client.request("/api/entries/", {"where": {"field": "title", "op": "contains", "values": ["%_"]}})["meta"]["total"] == 1
for op, value, count in [("eq", '{"group":1}', 7), ("json_contains", '{"group":1}', 7), ("json_contained_by", '{"group":1}', 7), ("json_key", "group", 14)]:
    assert client.request("/api/entries/", {"where": {"field": "payload", "op": op, "values": [value]}})["meta"]["total"] == count
assert client.request("/api/entries/", {"where": {"field": "payload", "op": "is_null"}})["meta"]["total"] == 1

facets = client.request("/api/entries/_facets", {"facet": "status", "limit": 1})["data"]["status"]
assert facets == {"buckets": [{"value": "active", "count": 5}], "other_count": 10}
facets = client.request("/api/entries/_facets", {"facet": "score,amount", "limit": 20})["data"]
assert {"value": None, "count": 2} in facets["score"]["buckets"]
assert facets["score"]["other_count"] == 0
assert all(isinstance(bucket["value"], str) for bucket in facets["amount"]["buckets"])
assert sum(bucket["count"] for bucket in client.request("/api/entries/_facets", {"facet": "status", "where": expression})["data"]["status"]["buckets"]) == len(expected_ids)

export_query = {"where": expression, "sort": "-score,title", "fields": "id,title,amount"}
csv_text = client.request("/api/entries/_export.csv", export_query)
reader = csv.DictReader(io.StringIO(csv_text))
assert reader.fieldnames == ["id", "title", "amount"]
assert list(reader) == client.request("/api/entries/", {**export_query, "page_size": 100})["data"]
for path in ["/api/entries/", "/api/entries/_aggregate", "/api/entries/_facets", "/api/entries/_export.csv"]:
    query = {"where": {"any": [{"field": "status", "op": "eq", "values": ["todo"]}, {"field": "secret", "op": "eq", "values": ["x"]}]}}
    if path.endswith("_facets"):
        query["facet"] = "status"
    client.request(path, query, status=403)
for query, status in [({"fields": "secret"}, 403), ({"fields": "title,title"}, 400), ({"sort": "score,score"}, 400), ({"page": 1, "limit": 3}, 400), ({"where": {"field": "score", "op": "contains", "values": ["1"]}}, 400)]:
    client.request("/api/entries/", query, status=status)
client.request("/api/entries/_facets", {"facet": "status,status"}, status=400)
client.request("/api/entries/_facets", {"facet": "payload"}, status=400)
client.request("/api/entries/_export.csv", {"page": 1}, status=400)

client.tenant = beta
assert client.request("/api/entries/")["meta"]["total"] == 0
assert client.request("/api/entries/_aggregate")["data"][0]["count"] == 0
assert client.request("/api/entries/_facets", {"facet": "status"})["data"]["status"] == {"buckets": [], "other_count": 0}
assert len(list(csv.reader(io.StringIO(client.request("/api/entries/_export.csv"))))) == 1
client.tenant = alpha

# Time buckets retain tenant and field authorization and use UTC boundaries.
for unit in ["day", "week", "month"]:
    result = client.request("/api/entries/_aggregate", {"group_by": "created_at", "time_bucket": unit})
    assert sum(row["count"] for row in result["data"]) == 15
    assert result["meta"]["order_by"] == "group" and result["meta"]["order"] == "asc"
    assert result["meta"]["time_bucket"] == unit
for query in [{"group_by": "status", "time_bucket": "day"}, {"group_by": "secret", "time_bucket": "month"}, {"group_by": "created_at", "time_bucket": "hour"}, {"order_by": "group"}, {"order_by": "invalid"}]:
    client.request("/api/entries/_aggregate", query, status=400)
client.tenant = beta
assert client.request("/api/entries/_aggregate", {"group_by": "created_at", "time_bucket": "day"})["data"] == []
client.tenant = alpha

# Import JSON as structured values, isolate invalid cells and reject duplicate headers.
stream = io.StringIO()
writer = csv.writer(stream)
writer.writerow(["title", "status", "amount", "payload", "booked_on"])
writer.writerow(["Import A", "todo", "9007199254740993", json.dumps({"imported": True}), "2026-12-31"])
writer.writerow(["Import B", "done", "9007199254740994", "{invalid", "2027-01-01"])
result = client.request("/api/entries/_import.csv", data=stream.getvalue())
assert result["succeeded"] == ["0"] and result["failed"][0]["id"] == "1"
assert result["failed"][0]["code"] == "invalid_row"
client.request("/api/entries/_import.csv", data="title,title,status\nA,B,todo\n", status=400)
imported = client.request("/api/entries/", {"q": "Import A"})["data"][0]
assert imported["payload"] == {"imported": True} and imported["amount"] == "9007199254740993"
stream = io.StringIO()
writer = csv.writer(stream)
writer.writerow(["title", "status", "amount", "payload", "booked_on"])
writer.writerow(["Import B", "done", "9007199254740994", json.dumps({"imported": True}), "2027-01-01"])
assert client.request("/api/entries/_import.csv", data=stream.getvalue())["succeeded"] == ["0"]
assert client.request("/api/entries/", {"q": "Import"})["meta"]["total"] == 2
trend = client.request("/api/entries/_aggregate", {"group_by": "booked_on", "time_bucket": "month", "q": "Import"})
assert trend["data"] == [{"count": 1, "group_booked_on": "2026-12-01"}, {"count": 1, "group_booked_on": "2027-01-01"}]
subprocess.run(["psql", os.environ["APPSTRUCT_E2E_DATABASE_URL"], "-v", "ON_ERROR_STOP=1"], input="UPDATE entries SET created_at = CASE WHEN title = 'Import A' THEN '2026-12-31 23:30:00-02'::timestamptz ELSE '2026-12-31 23:30:00+02'::timestamptz END WHERE title IN ('Import A', 'Import B');", text=True, check=True, stdout=subprocess.DEVNULL)
trend = client.request("/api/entries/_aggregate", {"group_by": "created_at", "time_bucket": "day", "q": "Import"})
assert trend["data"] == [{"count": 1, "group_created_at": "2026-12-31"}, {"count": 1, "group_created_at": "2027-01-01"}]

# A result above the export cap must fail before sending the CSV header.
subprocess.run(["psql", os.environ["APPSTRUCT_E2E_DATABASE_URL"], "-v", "ON_ERROR_STOP=1", "-v", f"tenant={alpha}"], input="INSERT INTO entries (id,title,status,tenant_id,revision) SELECT md5('overflow-' || i::text)::uuid, 'Overflow ' || i, 'todo', :'tenant'::uuid, 1 FROM generate_series(1,10001) i;", text=True, check=True, stdout=subprocess.DEVNULL)
client.request("/api/entries/_export.csv", status=400)
print("Query E2E passed: page totals, signed cursors, filters, facets, JSON CSV retry, UTC trends and tenant/field access.")
