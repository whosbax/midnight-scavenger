-- db/init.sql

-- ===================================================================
-- 🗄️ Midnight Scavenger - Database initialization script
-- ===================================================================
-- Ce script est exécuté automatiquement lors du premier démarrage du
-- conteneur Postgres. Il prépare les tables nécessaires au backend
-- de collecte de statistiques et de journalisation des interactions
-- avec les API Midnight.
-- ===================================================================

-- ========================
-- TABLE : stats
-- ========================
CREATE TABLE IF NOT EXISTS stats (
    id SERIAL PRIMARY KEY,
    container_id TEXT,
    miner_id TEXT,
--    wallet_addr TEXT NOT NULL,
    hash_rate DOUBLE PRECISION NOT NULL,
    timestamp TIMESTAMP NOT NULL DEFAULT NOW(),
    description TEXT
);

-- Index utiles
--CREATE INDEX IF NOT EXISTS idx_stats_wallet_addr ON stats(wallet_addr);
CREATE INDEX IF NOT EXISTS idx_stats_timestamp ON stats(timestamp);
CREATE INDEX IF NOT EXISTS idx_stats_miner_time ON stats(miner_id, timestamp DESC);

-- ========================
-- TABLE : api_return
-- ========================
CREATE TABLE IF NOT EXISTS api_return (
    id SERIAL PRIMARY KEY,
    container_id TEXT,
    miner_id TEXT,
    wallet_addr TEXT,
    endpoint TEXT NOT NULL,            -- ex: 'Register', 'Donate', 'Challenge', 'Solution'
    url TEXT NOT NULL,                 -- URL complète appelée (utile si environnement différent)
    timestamp TIMESTAMP NOT NULL DEFAULT NOW(),
    payload JSONB,                     -- corps JSON envoyé à l'API Midnight
    api_response JSONB,                -- réponse brute JSON renvoyée par l’API
    description TEXT                   -- message libre (succès, erreur, log, etc.)
);

-- Index utiles
CREATE INDEX IF NOT EXISTS idx_api_wallet_addr ON api_return(wallet_addr);
CREATE INDEX IF NOT EXISTS idx_api_endpoint_time ON api_return(endpoint, timestamp DESC);
CREATE INDEX IF NOT EXISTS idx_api_miner_endpoint ON api_return(miner_id, endpoint);
CREATE INDEX IF NOT EXISTS idx_api_timestamp ON api_return(timestamp);

-- ========================
-- VUE : dernier hashrate par mineur
-- ========================
CREATE OR REPLACE VIEW latest_hashrate AS
SELECT DISTINCT ON (container_id)
    container_id,
    miner_id,
    hash_rate,
    timestamp
FROM stats
ORDER BY container_id, timestamp DESC;

-- ========================
-- VUE : derniers appels API par endpoint
-- ========================
CREATE OR REPLACE VIEW latest_api_calls AS
SELECT DISTINCT ON (wallet_addr, endpoint)
    container_id,
    miner_id,
    wallet_addr,
    endpoint,
    timestamp,
    description,
    api_response
FROM api_return
ORDER BY wallet_addr, endpoint, timestamp DESC;

-- ===================================================================
-- ✅ Fin du script d'initialisation
-- ===================================================================
