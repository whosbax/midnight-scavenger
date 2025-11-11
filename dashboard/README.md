# Dashboard scavenger mine

Un tableau de bord minimaliste sur la journée les performances de “containers” de minage distribués (hash rate, solutions soumises, challenge…).


## construire le tableau de bord

```bash
docker compose up --build
```
## ouvrir le tableau de bord
http://localhost:3000/


| Champ                    | Description                                                                                                                                                                                                                             |
| ------------------------ | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **Hashrate moyen (H/s)** | Moyenne du hashrate d’un container sur les 10 dernières minutes.<br>Correspond à la moyenne des instantanés enregistrés dans cette fenêtre.                                                                                             |
| **Solutions /10 min**    | Nombre de solutions validées (endpoint `/solution`) soumises par le container durant les 10 dernières minutes.                                                                                                                          |
| **Solutions jour**       | Nombre de solutions validées depuis le début de la journée (00h UTC) pour ce container.                                                                                                                                                 |
| **% du global**          | Part que représente le hashrate moyen du container (`avg_h`) par rapport au total agrégé de tous les containers (`total_hrate_10_m`) dans la fenêtre 10 minutes.<br>Calcul : `(avg_h / total_hrate_10_m) × 100`, arrondi à 2 décimales. |
| **Hash estimé 10 min**   | Estimation du nombre de hachages effectués durant 10 minutes : `avg_h × 600` (600 secondes).                                                                                                                                            |
| **Moyenne jour (H/s)**   | Moyenne du hashrate du container depuis le début de la journée.                                                                                                                                                                         |
| **Total jour (H/s)**     | Somme des hashrates instantanés enregistrés depuis le début de la journée pour ce container.                                                                                                                                            |
| **Challenge ID**         | Identifiant du challenge en cours auquel le container a soumis une solution.                                                                                                                                                            |
| **Difficulté**           | Difficulté hexadécimale du challenge (ex. `000007FF`).                                                                                                                                                                                  |
| **Jour challenge**       | Numéro de jour du challenge (ex. jour 13).                                                                                                                                                                                              |
