# Connexion simple et exécution distante — conception validée

## Problème constaté

Le volume RWS expose les fichiers distants aux applications du Mac. Il ne
redirige pas leurs processus. La capture Delta montre un interpréteur Python
macOS lancé sous `/Users/...` pour créer un environnement dans le volume distant.
Le worktree examiné existe sur Linux, mais son fichier `.git` référence le
chemin absolu du montage Mac. Monter les fichiers ne suffit donc pas à fournir
un environnement de développement Linux.

## Approches

1. **Recommandée : connexion RWS simple et intégration distante explicite.**
   Centraliser les paramètres, exposer connexion/déconnexion/état, puis utiliser
   le mécanisme distant officiellement supporté par Delta s'il existe. Vérifier
   le lieu d'exécution depuis une commande réellement lancée par Delta.
2. **Instructions à l'agent pour appeler `rws exec`.** Facile à proposer, mais
   ne garantit pas que toutes ses commandes seront distantes. Ne pas présenter
   cette option comme une correction automatique ou une isolation.
3. **Agent exécuté sur la VM dans un shell SSH.** Garantit l'emplacement de
   ses processus, mais nécessite un agent disponible sur Linux et peut changer
   l'expérience utilisateur par rapport à Delta sur le Mac.

## Expérience proposée

- `rws connect NOM` monte avec les paramètres enregistrés ; une répétition
  réussit seulement si le montage existant est identifié comme celui attendu.
  Un montage étranger ou non identifiable est signalé, sans être réutilisé.
- `rws disconnect NOM` démonte normalement, vérifie la disparition du montage,
  et signale clairement un volume occupé. Aucun démontage forcé par défaut.
  Un volume déjà déconnecté est un résultat normal.
- `rws status [NOM]` distingue état du montage et accessibilité SSH. Il affiche
  hôte, chemin distant et chemin Mac, et précise que monter ne redirige pas
  les commandes des applications. Les sondes réseau ont un délai borné.
- Le backend FSKit et le chemin SSHFS expérimental sont enregistrés dans la
  configuration, pour éviter les variables et options à répéter. Préserver
  la lecture des configurations existantes et les commandes actuelles.
- Ajouter des raccourcis locaux Connexion/Déconnexion ouvrables par double clic,
  utilisant le même CLI et des chemins absolus. Garder ces fichiers personnels
  dans `.rws-local/` et y montrer les erreurs sans fermer immédiatement la fenêtre.

## Exécution et worktrees

Inspecter les capacités réellement disponibles de Delta avant de modifier son
intégration. N'annoncer « exécution VM » qu'après preuve depuis son outil de
commande : système Linux, hôte attendu, dossier distant attendu. Une commande
SSH lancée par RWS seul ne valide pas l'intégration Delta.

Si Delta propose un moteur distant, le configurer pour créer et gérer les
worktrees côté Linux. Si aucune intégration supportée n'est disponible,
documenter précisément cette limite et soumettre le choix d'un agent distant ;
ne pas remplacer silencieusement Delta ou détourner les exécutables du Mac.

Préserver le worktree en cours et ses modifications. Avant toute migration,
inspecter son état et les liens vers les métadonnées Git. Ne pas simplement
remplacer le chemin dans `.git`, ce qui peut casser son accès depuis Delta.
Ne pas supprimer l'environnement Python en cours : un environnement Linux
doit être créé par un processus Linux dans un emplacement distinct validé.

## Implémentation et erreurs

Réutiliser le transport SSH et le suivi du processus SSHFS existants. Isoler
la détection d'état et les opérations de connexion pour que le CLI et les
raccourcis partagent les mêmes règles. Ne jamais exécuter localement une
commande prévue pour la VM après une erreur SSH. Ne pas déconnecter le volume
Documents pendant un travail actif ; valider le cycle sur un montage dédié.

## Vérification

Tests automatisés : compatibilité des configurations, sélection des paramètres,
connexion répétée, montage étranger, déconnexion répétée, échec de démontage,
erreurs SSH et conservation des codes de sortie.

Vérification réelle sur données jetables : connecter, relire indépendamment
une modification par SSH, exécuter une commande Linux dans le bon sous-dossier,
déconnecter, vérifier la disparition du volume et la conservation des fichiers.

Vérification Delta séparée : commande initiée par Delta, identité Linux,
Git utilisable dans son worktree et environnement Python Linux. Consigner les
preuves et les limites dans `docs/validation.md`, les détails personnels dans
`.rws-local/diagnostics/`.

## Statut

Validée par la demande « implémente et teste ». Connexion, déconnexion, état,
paramètres mémorisés et raccourcis livrés et testés ; voir `docs/validation.md`.
La vérification utilise un fichier aléatoire jetable sur une racine distante
inscriptible. `connect --verify-existing` permet de vérifier un ancien montage
sans interruption. Delta ne fournit pas de moteur SSH documenté ; son worktree
et sa configuration n’ont pas été modifiés. Le choix d’une exécution native de
l’agent sur Linux reste distinct du montage et du shell RWS vérifiés.
