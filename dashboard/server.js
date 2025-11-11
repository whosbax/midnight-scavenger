// server.js
require('dotenv').config();
const express = require('express');
const path = require('path');
const { Pool } = require('pg');

const app = express();
const port = process.env.PORT || 3000;

const pool = new Pool({
  host: process.env.DB_HOST,
  port: Number(process.env.DB_PORT) || 5432,
  user: process.env.DB_USER,
  password: process.env.DB_PASS,
  database: process.env.DB_NAME,
  ssl: process.env.DB_SSL === 'true'
});

// Serve static files from root directory (since index.html is at root)
app.use(express.static(__dirname));

// For route "/" serve index.html explicitly
app.get('/', (req, res) => {
  res.sendFile(path.join(__dirname, 'index.html'));
});

// API endpoint
app.get('/api/stats', async (req, res) => {
  try {
    const result = await pool.query(`
      WITH h_avg_s AS (
        SELECT
          s.container_id AS ctn,
          AVG(s.hash_rate) AS avg_h,
          MAX(s.timestamp) AS last_t,
          COUNT(*) AS samples_count
        FROM stats s
        WHERE
          s.timestamp >= NOW() AT TIME ZONE 'utc' - INTERVAL '10 minutes'
        GROUP BY s.container_id
      ),
      global_stats AS (
        SELECT
          SUM(s.hash_rate) AS total_hrate_10_m,
          AVG(s.hash_rate) AS avg_hrate_10_m,
          MAX(s.hash_rate) AS max_hrate_10_m,
          COUNT(*) AS total_samples
        FROM stats s
        WHERE
          s.timestamp >= NOW() AT TIME ZONE 'utc' - INTERVAL '10 minutes'
      ),
      daily_stats AS (
        SELECT
          s.container_id AS ctn,
          AVG(s.hash_rate) AS day_avg_h,
          SUM(s.hash_rate) AS day_sum_h,
          COUNT(*) AS day_samples
        FROM stats s
        WHERE
          s.timestamp >= date_trunc('day', NOW() AT TIME ZONE 'utc')
        GROUP BY s.container_id
      ),
      solution_10m AS (
        SELECT
          a.container_id AS ctn,
          COUNT(*) AS submitted_nonces_10m
        FROM api_return a
        WHERE
          a.endpoint = '/solution'
          AND a.timestamp >= NOW() AT TIME ZONE 'utc' - INTERVAL '10 minutes'
        GROUP BY a.container_id
      ),
      solution_daily AS (
        SELECT
          a.container_id AS ctn,
          COUNT(*) AS submitted_nonces_day
        FROM api_return a
        WHERE
          a.endpoint = '/solution'
          AND a.timestamp >= date_trunc('day', NOW() AT TIME ZONE 'utc')
        GROUP BY a.container_id
      ),
      solution_extended AS (
        SELECT
          a.container_id,
          regexp_replace(a.url, '^.*?/solution/.*?/([^/]+)/.*$', '\\1') AS challenge_id
        FROM api_return a
        WHERE
          a.endpoint = '/solution'
          AND a.timestamp >= NOW() AT TIME ZONE 'utc' - INTERVAL '10 minutes'
      ),
      challenge_info AS (
        SELECT
          (a.api_response->'challenge'->>'challenge_id') AS challenge_id,
          (a.api_response->'challenge'->>'difficulty') AS difficulty,
          ((a.api_response->'challenge'->>'day')::int) AS challenge_day,
          (a.api_response->'challenge'->>'issued_at')::timestamp AS issued_at
        FROM api_return a
        WHERE
          a.endpoint = '/challenge'
          AND (a.api_response->'challenge'->>'challenge_id') IS NOT NULL
      )
      SELECT
        h.ctn,
        h.avg_h,
        ds.day_avg_h,
        ds.day_sum_h,
        ds.day_samples,
        COALESCE(s10.submitted_nonces_10m, 0) AS submitted_nonces_10m,
        COALESCE(sd.submitted_nonces_day, 0) AS submitted_nonces_day,
        g.total_hrate_10_m,
        g.avg_hrate_10_m,
        g.max_hrate_10_m,
        g.total_samples,
        ROUND((h.avg_h / NULLIF(g.total_hrate_10_m,0) * 100)::numeric, 2) AS pct_of_global_hrate,
        ROUND((h.avg_h * 600)::numeric) AS total_hashes_10m,
        se.challenge_id,
        ci.difficulty,
        ci.challenge_day,
        ci.issued_at
      FROM h_avg_s h
      LEFT JOIN daily_stats ds
        ON h.ctn = ds.ctn
      LEFT JOIN solution_10m s10
        ON h.ctn = s10.ctn
      LEFT JOIN solution_daily sd
        ON h.ctn = sd.ctn
      LEFT JOIN solution_extended se
        ON h.ctn = se.container_id
      LEFT JOIN challenge_info ci
        ON se.challenge_id = ci.challenge_id
      CROSS JOIN global_stats g
      GROUP BY
        h.ctn,
        h.avg_h,
        ds.day_avg_h,
        ds.day_sum_h,
        ds.day_samples,
        s10.submitted_nonces_10m,
        sd.submitted_nonces_day,
        g.total_hrate_10_m,
        g.avg_hrate_10_m,
        g.max_hrate_10_m,
        g.total_samples,
        se.challenge_id,
        ci.difficulty,
        ci.challenge_day,
        ci.issued_at
      ORDER BY
        h.avg_h DESC;
    `);
    res.json(result.rows);
  } catch (err) {
    console.error('DB query error', err);
    res.status(500).json({ error: 'Internal server error' });
  }
});

app.listen(port, () => {
  console.log(`Server is running at http://0.0.0.0:${port}`);
});
