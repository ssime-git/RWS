# Fonctionnalités restantes et couverture du besoin RWS

État audité le 20 septembre 2026. Le suivi du travail restant se fait dans les
[issues GitHub](https://github.com/ssime-git/RWS/issues), regroupées dans
[l’issue de suivi du besoin initial](https://github.com/ssime-git/RWS/issues/1).
Ce fichier conserve la description auditée du périmètre ;
[ROADMAP.md](ROADMAP.md) en donne l'ordre de travail et
[docs/validation.md](docs/validation.md) conserve les preuves et leurs limites.
Une fonctionnalité implémentée n'est pas considérée comme validée sans test du
parcours utilisateur correspondant. Les éléments ci-dessous ne sont pas livrés
sauf mention explicite d'une partie existante.

## Objectif initial — à ne pas réduire aux lanceurs explicites

**Ouvrir un projet distant monté sur le Mac dans Delta, ou ouvrir un terminal dans
ce dossier, et travailler comme sur la VM : les commandes liées à ce projet doivent
s'exécuter sur sa VM et dans le bon dossier, si cela est techniquement réalisable,
sans instructions spéciales à l'agent, sans préfixe `rws exec` à chaque commande et
sans liste d'exécutables à maintenir.**

Cela comprend les agents, scripts, tests, builds, outils Git et autres commandes
lancées pour le projet. L'interface de Delta ou du terminal peut rester sur le Mac ;
la cible est le lieu d'exécution des commandes du projet. Les processus Mac sans
rapport avec ce projet ne doivent pas être déplacés ou modifiés.

L'état actuel **ne satisfait pas cet objectif**. macFUSE/SSHFS expose les fichiers ;
les commandes distantes passent aujourd'hui par RWS/SSH explicitement, ou par des
instructions personnelles de Delta. Une bascule d'un shell interactif ne prouverait
pas que les outils de commande non interactifs et les sous-processus natifs de Delta
sont eux aussi distants. Ces deux parcours doivent être traités et vérifiés séparément.

La faisabilité d'une transparence générale reste à établir. Une limitation prouvée
doit être documentée et soumise comme choix de produit ; elle ne doit pas devenir
silencieusement une réduction de l'objectif ni être annoncée comme résolue.

## Inventaire priorisé

P0 : objectif central et garanties indispensables. P1 : usage quotidien fiable et
livraison simple. P2 : vision ultérieure déjà documentée, sans engagement de livraison
immédiate. « À étudier » n'est ni une promesse de faisabilité ni une fonctionnalité livrée.

| ID | Fonctionnalité restante | Priorité | État actuel |
| --- | --- | --- | --- |
| [RWS-001](https://github.com/ssime-git/RWS/issues/1) | Architecture de l'exécution transparente par contexte de projet | P0 | Faisabilité à établir |
| [RWS-002](https://github.com/ssime-git/RWS/issues/2) | Delta distant sans instructions spéciales aux agents | P0 | Absent ; instructions comme solution partielle |
| [RWS-003](https://github.com/ssime-git/RWS/issues/3) | Terminal ouvert dans un montage, ou `cd` dans celui-ci → VM | P0 | Absent |
| [RWS-004](https://github.com/ssime-git/RWS/issues/4) | Sous-dossiers, projets, worktrees et métadonnées Git cohérents | P0 | Mapping explicite partiel |
| [RWS-005](https://github.com/ssime-git/RWS/issues/5) | Échec distant sans poursuite locale silencieuse | P0 | Couvert seulement par les commandes RWS explicites |
| [RWS-006](https://github.com/ssime-git/RWS/issues/6) | Tout nouvel agent/outillage distant, sans liste codée en dur | P0 | Lanceur générique présent ; parcours transparent absent |
| [RWS-007](https://github.com/ssime-git/RWS/issues/7) | Preuve du lieu d'exécution dans les parcours réels | P0 | Diagnostics présents ; matrice incomplète |
| [RWS-008](https://github.com/ssime-git/RWS/issues/8) | Montage, démontage et favoris cohérents entre app et CLI | P1 | App partielle ; CLI sans épinglage |
| [RWS-009](https://github.com/ssime-git/RWS/issues/9) | Démarrage simple sur un Mac neuf | P1 | Détection présente ; installation/prérequis manuels |
| [RWS-010](https://github.com/ssime-git/RWS/issues/10) | Gestion complète de plusieurs espaces et hôtes | P1 | Ajout/liste présents ; édition/suppression UI manquantes |
| [RWS-011](https://github.com/ssime-git/RWS/issues/11) | App et parcours utilisateur validés de bout en bout | P1 | Vérifications fragmentaires et retours utilisateur |
| [RWS-012](https://github.com/ssime-git/RWS/issues/12) | Reprise fiable après réseau coupé, veille ou processus bloqué | P1 | Non validée globalement ; pas de reconnexion automatique |
| [RWS-013](https://github.com/ssime-git/RWS/issues/13) | Sauvegardes d'éditeurs et cohérence des fichiers | P1 | Opérations de base validées ; couverture incomplète |
| [RWS-014](https://github.com/ssime-git/RWS/issues/14) | Releases publiques et mises à jour réellement opérationnelles | P1 | Pipeline présent ; chaîne de production non validée |
| [RWS-015](https://github.com/ssime-git/RWS/issues/15) | Installation/mise à jour à emplacement stable | P1 | Builds locaux corrigés ; distribution à finaliser |
| [RWS-016](https://github.com/ssime-git/RWS/issues/16) | Sessions persistantes et reprise multiappareil | P2 | Absent |
| [RWS-017](https://github.com/ssime-git/RWS/issues/17) | Application iOS indépendante du Mac | P2 | Absent |
| [RWS-018](https://github.com/ssime-git/RWS/issues/18) | Windows/Linux et compatibilité plus large | P2 | App macOS Apple silicon seulement |
| [RWS-019](https://github.com/ssime-git/RWS/issues/19) | Capacités avancées : ports, transferts, notifications, cache | P2 | À préciser et prioriser ; absentes du prototype |
| [RWS-020](https://github.com/ssime-git/RWS/issues/20) | Documentation et préparation de distribution cohérentes | P1 | Dérives documentaires corrigées ici ; suivi permanent |

<a id="rws-001"></a>
## RWS-001 — Établir l'architecture de l'exécution transparente

Issue GitHub : [#1](https://github.com/ssime-git/RWS/issues/1).

- **Manque :** le mécanisme qui déduit la VM du contexte du projet avant le lancement
  d'une commande, sans dépendre du respect d'un prompt par un agent.
- **Travail :** examiner et prouver les capacités de backend/exécuteur distant des
  applications ciblées ; comparer une intégration de shell et les autres mécanismes
  techniquement disponibles. Évaluer séparément les commandes internes, les chemins
  absolus, les sous-processus et les shells non interactifs. Ne pas présupposer que
  macFUSE, un hook de shell ou une modification de PATH intercepte tout.
- **Acceptation :** décision d'architecture avec prototype, tableau des processus
  couverts/non couverts, contraintes et preuves. Si la cible universelle est impossible,
  décrire précisément les alternatives et obtenir un arbitrage avant de changer le besoin.
- **Liens :** débloque [002](#rws-002), [003](#rws-003), [005](#rws-005).
  La [conception historique](docs/superpowers/specs/2026-09-19-connect-remote-execution-design.md)
  documentait une solution explicite plus étroite ; elle ne vaut pas satisfaction du besoin initial.

<a id="rws-002"></a>
## RWS-002 — Delta sans instructions spéciales

Issue GitHub : [#2](https://github.com/ssime-git/RWS/issues/2).

- **Existant :** [règles personnelles Delta](src/agent_rules.rs),
  [procédure actuelle](docs/connection.md#delta-and-linux-commands) et un diagnostic
  réel réussi ; fonctionnement confirmé par l'utilisateur sur son parcours.
- **Manque :** exécution distante imposée par l'intégration, indépendamment de ces règles.
  Les terminaux intégrés, préparation de projet, outils Git, tests et sous-processus
  natifs doivent être évalués, pas seulement une commande volontairement enveloppée.
- **Acceptation :** nouveau projet/tâche dans Delta, règles RWS désactivées dans un
  profil de test sauvegardé, commande ordinaire lancée par l'agent → PID/exécutable,
  OS, hôte et cwd attendus sur la VM. Tester également les worktrees et un échec SSH.
  Documenter toute catégorie de commande restant locale. Ne pas altérer le profil actif pour tester.
- **Dépendances :** [001](#rws-001), [004](#rws-004), [005](#rws-005), [007](#rws-007).

<a id="rws-003"></a>
## RWS-003 — Terminaux et navigation dans un montage

Issue GitHub : [#3](https://github.com/ssime-git/RWS/issues/3).

- **Manque :** ouvrir Ghostty ou un autre terminal dans le montage, ou faire `cd`
  depuis un terminal local vers ce montage, ne déclenche aujourd'hui aucune session distante.
- **Acceptation :** ces deux entrées, y compris dans un sous-dossier, permettent
  `pwd`, `git`, `python`, un build et un agent ordinaire sur la bonne VM sans préfixe RWS.
  Tester les shells réellement pris en charge, leur démarrage et les commandes composées
  (`cd … && commande`), pipes, redirections, variables, Ctrl+C et dimensions du terminal.
- **À définir :** comportement de `exit`, du retour au contexte local, du passage à
  un autre espace/hôte et des terminaux déjà ouverts ; activation/désactivation réversible.
  Une intégration zsh ne doit pas être présentée comme une interception universelle.
- **Dépendances :** [001](#rws-001), [004](#rws-004), [005](#rws-005).

<a id="rws-004"></a>
## RWS-004 — Contexte exact et worktrees

Issue GitHub : [#4](https://github.com/ssime-git/RWS/issues/4).

- **Existant :** [résolution des chemins et contexte Git](src/routing.rs), mapping
  explicite des répertoires et certaines métadonnées de worktrees Delta.
- **Manque :** application de ce contexte dans tous les futurs parcours transparents.
  Le bouton d'agent démarre à la racine de l'espace, pas dans un sous-dossier choisi.
- **Acceptation :** projets différents, deux hôtes, chemins avec espaces/accents,
  sous-dossiers, liens symboliques et worktrees → bon hôte/cwd/Git. Une sortie du
  périmètre doit être explicite ; aucun environnement Python macOS créé involontairement
  dans le projet Linux. Préserver les métadonnées et modifications des worktrees existants.
- **Dépendance :** [001](#rws-001). Preuves existantes : [validation](docs/validation.md).

<a id="rws-005"></a>
## RWS-005 — Ne jamais poursuivre localement par accident

Issue GitHub : [#5](https://github.com/ssime-git/RWS/issues/5).

- **Existant :** les [commandes explicites RWS](src/main.rs) remplacent leur processus
  par SSH ; tests de code de sortie et absence de repli local pour le lanceur.
- **Manque :** cette garantie pour [002](#rws-002) et [003](#rws-003).
- **Acceptation :** VM indisponible, authentification refusée, agent absent, montage
  disparu ou non identifié, interruption SSH → erreur visible ; aucun exécutable
  local de même nom lancé et aucune écriture locale par une commande prévue distante.
  Tester avec témoins locaux et dossiers jetables, sans désactiver les vérifications SSH.

<a id="rws-006"></a>
## RWS-006 — Agents et outils génériques

Issue GitHub : [#6](https://github.com/ssime-git/RWS/issues/6).

- **Existant :** [lanceur générique](macos/Sources/RWSApp/AgentLauncher.swift), commande
  `rws agent`, chargement du profil distant, versions de plusieurs agents vérifiées.
  L'utilisateur confirme le lancement de Claude depuis l'app.
- **Manque :** même comportement depuis les parcours transparents ; validation d'un
  nouvel agent après installation sur la VM, sans modification de RWS.
- **Acceptation :** exécutable arbitraire nommé ou par chemin absolu, environnement et
  dépendances distants, arguments et session interactive ; vérifier les agents envisagés
  (Hermes, Cline, OpenClaw) selon leurs modes réellement disponibles. Une extension
  d'éditeur seule nécessite une intégration distincte, pas une promesse de compatibilité CLI.
- **Dépendances :** [002](#rws-002), [003](#rws-003), [005](#rws-005).
  Installation, authentification et tests d'inférence sont séparés du simple démarrage.

<a id="rws-007"></a>
## RWS-007 — Preuves et matrice de compatibilité

Issue GitHub : [#7](https://github.com/ssime-git/RWS/issues/7).

- **Manque :** une matrice exécutable reliant chaque promesse à son entrée réelle
  (Delta, terminal contextuel, `cd`, app, CLI), shell, application et système testés.
- **Acceptation :** observer hôte/OS/cwd/PID/exécutable côté VM et témoins côté Mac ;
  un chemin `/Volumes/...` ou un simple `--version` ne prouve pas tout le workflow.
  Inclure un test de commande émise par un agent, sans exposer de secrets.
- **Liens :** condition de clôture de [002](#rws-002) à [006](#rws-006) ;
  étend [docs/validation.md](docs/validation.md), [tests CLI](tests/cli.rs) et
  [tests de routage](tests/routing.rs).

<a id="rws-008"></a>
## RWS-008 — Cycle de montage et favoris

Issue GitHub : [#8](https://github.com/ssime-git/RWS/issues/8).

- **Existant :** montage vérifié, démontage normal, gestion du favori dans
  [l'app](macos/Sources/RWSApp/AppModel.swift) et
  [renouvellement du bookmark](macos/Sources/RWSApp/FinderSidebar.swift).
- **Manque :** comportement cohérent pour les entrées app/CLI, éjection par Finder,
  montage perdu et reconnexion. Le CLI ne gère pas actuellement les favoris.
  L'utilisateur accepte le recours manuel avec un message clair : ne pas bloquer P0 pour cela.
- **Acceptation :** opérations répétées, échec d'épinglage, aucun doublon, préservation
  des autres favoris, volume occupé non forcé, clic après remontage. Définir clairement
  le comportement hors connexion ; un favori actuel ne déclenche pas un montage.
- **Liens :** [test du cycle](macos/Tests/RWSAppTests/MountLifecycleLiveTests.swift),
  [tests Finder](macos/Tests/RWSAppTests/FinderSidebarTests.swift), [guide](docs/finder-macos.md).

<a id="rws-009"></a>
## RWS-009 — Installation et détection sur un Mac neuf

Issue GitHub : [#9](https://github.com/ssime-git/RWS/issues/9).

- **Existant :** [détection limitée aux emplacements connus](macos/Sources/RWSApp/Startup.swift),
  erreurs de dépendances, [modèle privé](.rws-local.template/README.md).
- **Manque :** parcours simple pour obtenir le SSHFS compatible, installer/configurer
  les prérequis et activer FSKit ; détection seule ≠ installation ou activation.
- **Acceptation :** Mac neuf sans fichiers privés du développeur, app à son emplacement
  final, configuration SSH valide, premier montage et lancement distant. Dépendances
  absentes/incompatibles → instructions exactes ; aucune désactivation implicite des protections macOS.
- **Liens :** [guide de setup](.agents/skills/rws-macos-setup/SKILL.md), [014](#rws-014), [015](#rws-015).

<a id="rws-010"></a>
## RWS-010 — Gestion réutilisable des espaces et hôtes

Issue GitHub : [#10](https://github.com/ssime-git/RWS/issues/10).

- **Manque :** édition/suppression d'espaces dans l'app, diagnostics simples de
  connexion/authentification, validation sur un deuxième hôte et plusieurs projets.
- **Acceptation :** ajouter/modifier/retirer un espace sans éditer le JSON ; conserver
  les autres espaces et leurs données ; traitement explicite d'un espace monté ;
  pas de chemin, hôte ou nom de projet personnel codé en dur.
- **Liens :** [configuration](src/config.rs), [app](macos/Sources/RWSApp/AppModel.swift), [004](#rws-004).

<a id="rws-011"></a>
## RWS-011 — Parcours app réellement validé

Issue GitHub : [#11](https://github.com/ssime-git/RWS/issues/11).

- **Manque :** validation complète et reproductible de l'interface, et états visibles
  distinguant montage, SSH, lieu d'exécution et erreur. L'outil de contrôle utilisé a
  échoué à lire la fenêtre RWS ; les tests CLI/helpers ne remplacent pas cette preuve.
- **Acceptation :** depuis l'ouverture de l'app jusqu'au montage, clic favori,
  lancement distant et démontage, avec erreurs et relancement. Inclure les scénarios
  confirmés manuellement par l'utilisateur sans les confondre avec des tests automatisés.
- **Liens :** [interface](macos/Sources/RWSApp/ContentView.swift), [007](#rws-007), [008](#rws-008), [009](#rws-009).

<a id="rws-012"></a>
## RWS-012 — Réseau, veille et récupération

Issue GitHub : [#12](https://github.com/ssime-git/RWS/issues/12).

- **Manque :** validation des interruptions et politique claire de reconnexion,
  traitement des processus bloqués/états périmés et diagnostic après veille.
- **Acceptation :** essais sur données jetables, délais bornés et erreurs visibles,
  reprise sans faux état « connecté », sans démontage forcé silencieux ni redémarrage
  global des services. Ne pas annoncer une résistance aux corruptions sans preuve.
- **Liens :** [cycle CLI](src/lifecycle.rs), [runner app](macos/Sources/RWSApp/ProcessRunner.swift),
  [005](#rws-005), [008](#rws-008), [013](#rws-013).

<a id="rws-013"></a>
## RWS-013 — Sauvegardes et visibilité des fichiers

Issue GitHub : [#13](https://github.com/ssime-git/RWS/issues/13).

- **Manque :** couverture complète des sauvegardes d'éditeurs, remplacements atomiques,
  changements concurrents/distants et interruptions d'écriture.
- **Acceptation :** éditer/sauver/rouvrir depuis les applications ciblées, comparer les
  octets par SSH, tester Unicode/liens/remplacements et remontage. Mesurer les délais
  de visibilité ; ne pas déduire la fiabilité des sauvegardes d'un simple copier-coller Finder.
- **Liens :** [SSHFS corrigé](docs/sshfs-fskit.md), [validation](docs/validation.md), [012](#rws-012).

<a id="rws-014"></a>
## RWS-014 — Releases et mises à jour utilisateurs

Issue GitHub : [#14](https://github.com/ssime-git/RWS/issues/14).

- **Existant :** [CI de release](.github/workflows/release.yml),
  [intégration Sparkle](macos/Sources/RWSApp/UpdateManager.swift).
- **Manque :** compte/certificats Apple Developer, secrets de publication,
  release signée/notarisée et mise à jour réelle entre deux versions. Les builds de
  développement désactivent les mises à jour ; aucune MAJ automatique opérationnelle n'est revendiquée.
- **Acceptation :** publication complète, détection/téléchargement vérifiés,
  installation après déconnexion des volumes, configuration préservée, reconnexion,
  échec de téléchargement/signature testé. Clarifier détection automatique et
  installation explicite ; macFUSE est une dépendance mise à jour séparément.
- **Liens :** [procédure](docs/macos-app.md), [009](#rws-009), [015](#rws-015).

<a id="rws-015"></a>
## RWS-015 — Une app identifiable, sans anciennes copies concurrentes

Issue GitHub : [#15](https://github.com/ssime-git/RWS/issues/15).

- **Existant :** [build avec remplacement au même chemin](scripts/build-macos-app.sh),
  [publication et rollback](scripts/publish-macos-build.py), anciennes versions en ZIP,
  révision et date de build visibles ; refus de remplacer un bundle en cours d'exécution.
- **Manque :** validation de l'installation finale et de ses raccourcis/Dock, puis des
  mises à jour distribuées ; les builds locaux ne sont pas un installateur utilisateur.
- **Acceptation :** lancement de la version attendue après plusieurs installations/MAJ,
  aucune suppression manuelle régulière ni perte de configuration ; reprise sur échec.
- **Dépendances :** [009](#rws-009), [014](#rws-014).

<a id="rws-016"></a>
## RWS-016 — Sessions persistantes (vision différée)

Issue GitHub : [#16](https://github.com/ssime-git/RWS/issues/16).

- **Manque :** agents/commandes survivant à une déconnexion du client et réattachement
  depuis un autre appareil. Une session SSH actuelle n'apporte pas cette garantie.
- **Acceptation :** choisir le mécanisme distant, reconnecter à la même session après
  coupure, préserver sortie et état, interruption volontaire fonctionnelle.
- **Liens :** [décisions](docs/decisions.md), [012](#rws-012), [017](#rws-017).
  Aucun service distant supplémentaire n'est installé par cette liste.

<a id="rws-017"></a>
## RWS-017 — iOS indépendant (vision différée)

Issue GitHub : [#17](https://github.com/ssime-git/RWS/issues/17).

- **Manque :** application pour fichiers/transferts, commandes et suivi d'agents,
  sans relais nécessitant un Mac allumé.
- **Acceptation :** connexion directe autorisée à la VM, navigation/transfert et
  reprise de session lorsque le Mac est éteint ; politique de cache/authentification définie.
- **Dépendance :** [016](#rws-016) pour la reprise des sessions.

<a id="rws-018"></a>
## RWS-018 — Plateformes et compatibilité (vision différée)

Issue GitHub : [#18](https://github.com/ssime-git/RWS/issues/18).

- **Manque :** clients Windows/Linux, package macOS Intel et matrice réelle des
  versions de macOS/backends supportés. La cible de compilation n'est pas une preuve FSKit.
- **Acceptation :** définir puis tester chaque combinaison annoncée sur une machine
  propre ; aucune compatibilité déduite du seul passage des tests du cœur Rust.
- **Dépendances :** [007](#rws-007), [009](#rws-009), [012](#rws-012).

<a id="rws-019"></a>
## RWS-019 — Capacités avancées à cadrer (vision différée)

Issue GitHub : [#19](https://github.com/ssime-git/RWS/issues/19).

- **Manque :** mécanisme RWS de ports de développement, transferts avancés,
  notifications de changements, cache borné/mode hors ligne et éventuel volume unifié.
- **Statut :** sujets présents dans les limites/roadmap historiques ; leur périmètre
  et priorité doivent être confirmés avant implémentation. Pas de garantie déduite de SSHFS.
- **Acceptation à définir :** besoins concrets pour chaque capacité, mesures et tests
  associés ; décider notamment comment accéder depuis le Mac à un serveur lancé sur la VM.
- **Liens :** [limites du prototype](docs/prototype.md#limits), [012](#rws-012), [013](#rws-013).

<a id="rws-020"></a>
## RWS-020 — Cohérence documentaire et distribution

Issue GitHub : [#20](https://github.com/ssime-git/RWS/issues/20).

- **Manque :** maintien d'une seule définition du périmètre, couverture liée aux tests,
  décision de licence avant présentation comme logiciel open source licencié.
- **Acceptation :** README, roadmap et guides distinguent cible, implémentation et
  validation ; aucune mention « terminé » tant que [001–007](#inventaire-priorisé)
  ne couvrent pas le parcours central ou qu'un périmètre différent n'a pas été accepté.
  Conserver les configurations réelles et diagnostics hors Git.
- **Liens :** [README](README.md), [ROADMAP](ROADMAP.md),
  [CONTRIBUTING](CONTRIBUTING.md), [journal de validation](docs/validation.md).

## Règle de clôture

Pour chaque ID : changement lié, critères d'acceptation exécutés, preuve datée et
limites connues. Les commandes explicites et les instructions Delta restent des
solutions disponibles, mais ne clôturent ni [RWS-002](#rws-002) ni [RWS-003](#rws-003).
Cette liste documente le travail restant ; elle ne déclenche ni installation,
changement des shells personnels, nouvelle intégration ni déploiement.
