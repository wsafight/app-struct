INSERT INTO entries (id, tenant_id, code, title, secret, created_at)
SELECT gen_random_uuid(), :'alpha'::uuid, 'alpha-' || n,
       'Benchmark entry ' || n, 'restricted-alpha', CURRENT_TIMESTAMP
FROM generate_series(1, :rows) AS n;

INSERT INTO entries (id, tenant_id, code, title, secret, created_at)
SELECT gen_random_uuid(), :'beta'::uuid, 'beta-' || n,
       'Other tenant ' || n, 'restricted-beta', CURRENT_TIMESTAMP
FROM generate_series(1, :rows) AS n;

ANALYZE entries;
