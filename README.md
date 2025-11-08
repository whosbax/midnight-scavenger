
---

# 🧠 Midnight Scavenger Miner

`Midnight Scavenger` est un **miner distribué** écrit en **Rust**, conçu pour exécuter plusieurs instances parallèles via **Docker Compose**, avec une gestion automatique des wallets, des seeds et des configurations locales persistantes.

Chaque conteneur (`miner-1`, `miner-2`, etc.) fonctionne de manière isolée tout en partageant certaines ressources globales (ex : liste de dons).

---

## ⚙️ Fonctionnalités principales

* 🪙 **Génération automatique de wallets Ed25519** avec seed BIP39 (jamais écrasée).
* 💾 **Persistance complète** des clés et seeds dans des répertoires dédiés (`/usr/local/bin/config/...`).
* 🔄 **Gestion intelligente des locks** : empêche plusieurs conteneurs d’utiliser le même dossier.
* 📦 **Support multi-instance** via Docker Compose (`--scale miner=N`).
* 🤝 **Liste commune `donate_to`** : générée une fois si absente (3 adresses par défaut).
* 🧹 (à venir) Nettoyage automatique des locks orphelins après redémarrage.

---

## 😂 Règles d'or /!\

Les seeds BIP39 sont sacrées : une fois écrites, elles ne sont jamais écrasées ni supprimées par le programme.

Si tu veux éviter qu’on vienne t’apprendre le piano à domicile (oui, menace musicale — efficace et non violente), envoie quelques sats vers l’adresse suivante et tout ira bien :

`addr1q8cd35r4dcrl4k4prmqwjutyrl677xyjw7re82x6vm4t7vtmrd3ueldxpq74m47dtr03ppesr5ral6plt7acy5gjph5surek0h`

Cette adresse sera automatiquement ajoutée au fichier config/donate_list.txt si la liste n’existe pas.

---

## 📁 Structure du projet

```
.
├── Dockerfile
├── docker-compose.yml
├── Cargo.toml
├── src/
│   ├── main.rs
│   ├── wallet.rs
│   ├── ...
└── config/
    ├── donate_list.txt             # liste d’adresses de donation (commune)
    ├── donate_list_seed.txt        # seeds correspondantes (uniquement si la donate_list est générée)
    ├── 1/
    │   ├── in_use.lock
    │   └── miner-1/wallets/
    │       ├── keys.hex
    │       └── seeds.txt
    ├── 2/
    │   ├── in_use.lock
    │   └── miner-2/wallets/
    │       ├── keys.hex
    │       └── seeds.txt
    └── ...
```

---

## ⚙️ Variables d’environnement principales

| Variable                  | Description                                                                                     | Exemple / Valeur par défaut                                           |
|---------------------------|-------------------------------------------------------------------------------------------------|-----------------------------------------------------------------------|
| `APP_LOG_LEVEL`           | Niveau de logging spécifique à l’app                                                           | `"info"`                                                              |
| `MINER_THREADS`           | Nombre de threads pour le miner                                                                | `100`                                                                 |
| `MAX_WALLETS_PER_INSTANCE`| Nombre maximal de wallets par instance                                                        | `2`                                                                   |
| `ENABLE_STATS_BACKEND`    | Activer l’envoi des stats vers le backend                                                     | `true`                                                                |
| `POSTGRES_HOST`           | Adresse du serveur PostgreSQL                                                                  | `stats-db`                                                            |
| `POSTGRES_PORT`           | Port PostgreSQL                                                                                | `5432`                                                                |
| `POSTGRES_USER`           | Utilisateur PostgreSQL                                                                         | `stats`                                                               |
| `POSTGRES_PASSWORD`       | Mot de passe PostgreSQL                                                                        | `stats_pass`                                                          |
| `POSTGRES_DB`             | Nom de la base de données PostgreSQL                                                           | `stats`                                                               |
| `BACKEND_HOST`            | Adresse du service backend pour les stats                                                     | `stats-backend`                                                       |
| `BACKEND_PORT`            | Port du service backend pour les stats                                                       | `8080`                                                                |
| `STATS_BACKEND_URL`       | URL complète pour l’API d’insertion de stats                                                 | `http://$BACKEND_HOST:$BACKEND_PORT/insert_stat`                      |
| `API_BACKEND_URL`         | URL complète pour l’API d’insertion de retours API                                           | `http://stats-backend:$BACKEND_PORT/insert_api_return`                |
| `STATS_REPORT_INTERVAL`   | Intervalle en secondes pour le reporting des stats                                           | `10`                                                                  |
| `DATABASE_URL`            | URL de connexion complète à PostgreSQL pour l’application                                   | `postgres://$POSTGRES_DB:$POSTGRES_PASSWORD@$POSTGRES_HOST:$POSTGRES_PORT/stats` |
| `STATS_BEARER_TOKEN`      | Token Bearer pour authentification vers le backend de stats                                  | `AZERTY`                                                              |

---

## 🏗️ Installation & lancement

### Prérequis
* Installer Docker et Docker Compose
* Avoir Rust et Cargo installés si compilation locale nécessaire
* Cloner ton dépôt Git localement

```bash
git clone https://github.com/whosbax/midnight-scavenger.git
cd midnight-scavenger
```

### 1. Construire l’image Docker

```bash
docker compose build
```

### 2. Lancer une ou plusieurs instances de mineur

Plusieurs instances simultanées:

```bash
docker compose up miner --build --scale miner=2 -d
```

Chaque instance utilisera automatiquement un **répertoire isolé**, par exemple :

```
/usr/local/bin/config/1/miner-1
/usr/local/bin/config/2/miner-2
```

Les locks `in_use.lock` assurent qu’aucun dossier n’est partagé entre deux conteneurs.

### Optionnel: Persistance base de données: stats, hashrate, retour API Midnight
```bash
docker compose up  stats-db
```
---

## 💰 Liste de donation commune

Au premier lancement, si le fichier `donate_list.txt` n’existe pas, il est automatiquement créé :

* `donate_list.txt` → contient les **adresses publiques** à utiliser dans les appels `donate_to`.
* `donate_list_seed.txt` → contient les **seeds** correspondantes (pour régénération future).

Ces fichiers sont partagés par toutes les instances.

---


## 🧠 Bonnes pratiques

*  **Tu peux fournir t'as propre `donate_list.txt` sans ajouter tes clés privés.**
* *⚠️ si tu ne fourinit pas `donate_list.txt`, ne supprime **jamais** les fichiers `donate_list_seed.txt` : ils contiennent les phrases BIP39 de tes wallets.*
* 🔁 Tu peux supprimer les `.lock` manuellement en cas d’arrêt brutal des conteneurs (fonction de nettoyage en cours).
* 🧱 Pour augmenter le nombre d’instances, ajuste simplement le nombre de conteneur:

  ```bash
  docker compose up --scale miner=5
  ```

---

## 🧰 Technologies

* 🦀 **Rust**
* 🔐 `ed25519-dalek`, `bip39`, `blake2`
* ⚡ `tokio` pour l’asynchronicité
* 🧩 `tracing` pour la journalisation avancée
* 🐳 Docker / Docker Compose pour l’orchestration multi-instance

---

## Retrouve nous sur discord

https://discord.gg/syWbjztX

`addr1q8cd35r4dcrl4k4prmqwjutyrl677xyjw7re82x6vm4t7vtmrd3ueldxpq74m47dtr03ppesr5ral6plt7acy5gjph5surek0h` 



---

## 📊 Requêtes SQL pour le hashrate

Le tableau `stats` enregistre le hashrate de chaque mineur dans chaque conteneur.  
Les colonnes importantes pour le suivi du hashrate sont :  

- `container_id` : identifiant du conteneur / machine.  
- `miner_id` : identifiant du mineur dans le conteneur.  
- `hash_rate` : nombre de H/s mesurés pour l’intervalle donné.  
- `timestamp` : date et heure de la mesure.  

---

### 1️⃣ Stats:

```sql
-- Hashrate et activité API combinés par conteneur et mineur
SELECT
    s.container_id,
    s.miner_id,
    SUM(s.hash_rate) AS total_hashrate_hs,
    COUNT(a.id) AS total_api_calls,
    MAX(s.timestamp) AS last_hashrate_update,
    MAX(a.timestamp) AS last_api_call
FROM stats s
LEFT JOIN api_return a
    ON s.container_id = a.container_id
    AND s.miner_id = a.miner_id
GROUP BY s.container_id, s.miner_id
ORDER BY total_hashrate_hs DESC, total_api_calls DESC;


-- Calcule le hashrate moyen par seconde pour chaque mineur :
SELECT 
    container_id,
    miner_id,
    AVG(hash_rate) AS avg_hashrate_hs,
    MAX(timestamp) AS last_update
FROM stats
GROUP BY container_id, miner_id
ORDER BY container_id, miner_id;

-- Hashrate moyen sur les 5 dernières minutes
SELECT 
    container_id,
    miner_id,
    AVG(hash_rate) AS avg_hashrate_hs,
    MAX(timestamp) AS last_update
FROM stats
WHERE timestamp >= NOW() - INTERVAL '5 minutes'
GROUP BY container_id, miner_id
ORDER BY container_id, miner_id;


-- hashrate total combiné de tous les mineurs dans chaque conteneur 
SELECT 
    container_id,
    SUM(hash_rate) AS total_hashrate_hs,
    MAX(timestamp) AS last_update
FROM stats
GROUP BY container_id
ORDER BY container_id;
```


---