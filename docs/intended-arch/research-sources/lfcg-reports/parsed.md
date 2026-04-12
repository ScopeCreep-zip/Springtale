## Left_4_Dead_2

- Author: Pedro Pais
- Created: 2024-05-02
- Game mode: coopCampaign
- Analysis level: macro
- Analysis type: played
- Framework difficulty (1-5): 3

**Codings:**

- **Party** — *The whole party progresses together (even if the save is stored in the device of one of the players)*
    - Party progression results from the contributions of a group of players that opted to play together, where part of the progress is solely associated with that group (not shared with the rest of the ser
- **Party Creation** — *Players have to organise themselves before playing*
    - Party Creation encompasses all games where groups are defined by the players themselves prior to initiating gameplay, typically leveraging friend lists or co-located party creation.
- **Shared** — *Players pursue the same goal (finishing the level/game)*
    - Shared goals consist of singular objectives that multiple players pursue and work together to achieve
- **Individuals** — *There is no mechanic that relies on the relationship between players.*
    - Individuals describes when players’ entities have no established relationship with each other, which affects gameplay.
- **Shared**
    - Shared world views apply to a game when players have access to the same world.
- **Distinct**
    - Distinct refers to when each player has control of their own individual viewpoint, typically through separate screens.
- **Single** — *Players control their own character, and nothing else.*
    - Single applies to any game where players have control of one single entity.
- **Arbitrary** — *No selection exists that the game assumes (it attributes specific characters to players 1 and 2)*
    - Arbitrary means that all player entities are equal in their ability to act upon the world. If a selection exists, it is merely an aesthetic choice.
- **Static** — *Outside of tutorial sections, a player's character can always do the same thing (i.e., shoot portals, pick up interactables)*
    - When the player's entity is constant throughout gameplay.
- **Free Collaboration**
    - In free collaboration, players can take on available tasks as they wish and decide how to contribute.
- **Coincident Collaboration** — *Some tasks ask players to do the same thing at the same time (e.g., hold a point)*
    - In coincident collaboration, players are accomplishing the same task together. 
- **Concurrent Collaboration**
    - In concurrent collaboration, players have to perform gameplay tasks concurrently.
- **Sequential Collaboration**
    - In sequential collaboration, players have to do tasks in sequential order, with tasks typically assigned to different players.
- **Required/Incentivised**
    - When Required/Incentivised, the game challenges require players to communicate (e.g. when there is essential asymmetric information), or incentivised through challenges that are made easier through it
- **Pings**
    - Support for players to communicate by signaling specific things in their view (e.g. aiming at something and signaling)
- **Voice Lines**
    - Players can communicate through specific voice lines their characters have (e.g. "Enemy over there!")
- **In-Game Movement/Actions**
    - Players can communicate through specific movement or actions (e.g. shooting a weapon on the ground to notify others to pick it up)
- **Spatial** — *The game's enemies deliberately focus isolated players*
    - Spatial, inspired by Reuter et al. 2014, happens when the game forces or incentivises players to be at a certain distance from one another. This can happen in a variety of ways, for example, by creati
- **Assistive Actions** — *Players can heal each other*
    - Assistive Actions encompass actions that one player performs to the benefit of other/s (i.e. note that the player can have indirect benefits such as reviving others to increase their own chances of su
- **Consumables** — *Ammo and healing items are shared*
    - Consumables refer to items that exist in a limited amount and can be utilised by players to invoke an effect (usually existing as a form of currency, materials, or food) or are not consumed on purpose
- **Interactables**
    - Interactables refer to virtual objects and non-player characters/entities within the environment that respond to players’ actions. Interactables can be moved, shaped, or activated (e.g., a cannon that
- **Space** — *Some items are explosive and can damage friendly players*
    - Space is also a resource as it defines the various places in the environment that a player can occupy and interact with. Space is shared when its utilisation is affected or constrained by others' pres

---

## Deep_Rock_Galactic

- Author: Tiago Pereira
- Created: 2025-10-19
- Game mode: coopCampaign
- Analysis level: macro
- Analysis type: observations
- Framework difficulty (1-5): 3
- Comments: A framework LFCG é muito prática para sistematizar a 
análise de jogos cooperativos complicados, e permite identificar padrões de dependência, comunicação e 
assimetria. Eu diria que a principal dificuldade é interpretar corretamente algumas categorias sem jogar o 
jogo diretamente, especialmente em jogos com muitas camadas de progressão tanto individual como 
coletiva. Talvez fosse útil ter exemplos e diretrizes sobre como propriamente observar indiretamente os 
padrões de cooperação.

**Codings:**

- **Party** — *As missões e progressão em objetivos primários são realizados coletivamente em equipa.*
    - Party progression results from the contributions of a group of players that opted to play together, where part of the progress is solely associated with that group (not shared with the rest of the ser
- **Individual** — *Apesar da missão ser compartilhada em equipa, cada jogador tem progressão individual, 
o que inclui XP, loot e desenvolvimento da personagem.*
    - Individual progression is defined by individual choice and the individual impact of the progression. In-game activities can vary from individual to fully cooperative but progression affects the indivi
- **Party Creation** — *Antes do jogo começar propriamente, os jogadores formam equipas de até 4 mineiros, 
e selecionam classes e os papéis que vão desempenhar.*
    - Party Creation encompasses all games where groups are defined by the players themselves prior to initiating gameplay, typically leveraging friend lists or co-located party creation.
- **Drop-in/Drop-out** — *Existe a possibilidade de jogadores entrarem e saírem a meio das partidas (desde 
que a missão o permita).*
    - Drop-in/Drop-out encompasses games where there is the ability for players to join in the midst of the gameplay and drop out at any point.
- **Looking for Group** — *Se o jogador não tem uma equipa, existe a opção de matchmaking online, o que 
permite que ele procure por grupos de jogadores para formar uma equipa e realizar uma missão.*
    - Looking for Group happens when there are grouping mechanisms that allow players to formally look for and create a group/party. This is typically achieved through matchmaking or looking for group queue
- **Shared** — *Todos os jogadores partilham o objetivo comum de sobreviver, minerar recursos e completar a missão.*
    - Shared goals consist of singular objectives that multiple players pursue and work together to achieve
- **Intertwined** — *Todas as ações executadas por cada jogador têm impacto direto no progresso dos 
outros jogadores, se não houver cooperação, a missão falha.*
    - Intertwined goals determine individual objectives assigned to different players that, in some way, are dependent on each other. The dependency may be uni- or bidirectional. If the dependency is bidire
- **Independent** — *Pode-se argumentar que existem objetivos secundários, tais como:
- XP, loot e upgrades;
Mas a missão principal é compartilhada entre todos os jogadores.*
    - Independent goals define individual goals that do not directly interact with other players,  typically different from other players.
- **Allies** — *Todos os jogadores cooperam para finalizar a missão com sucesso, através da contribuição 
de habilidades complementares das 4 classes pré-selecionadas antes da partida começar.*
    - Allies, when players are part of a shared faction/race etc, which causes players to have special gameplay mechanics between each other.
- **Shared** — *Todos os jogadores interagem no mesmo mapa e ambiente de jogo, e também partilham os 
mesmos inimigos, recursos e obstáculos.*
    - Shared world views apply to a game when players have access to the same world.
- **Distinct** — *Cada jogador tem o seu próprio ecrã online, apesar de todos participarem no mesmo mapa 
e ambiente de jogo.*
    - Distinct refers to when each player has control of their own individual viewpoint, typically through separate screens.
- **Distinct** — *Cada jogador controla uma classe única com habilidades próprias da mesma (Gunner, 
Enginner, Driller, Scout).*
    - Distinct means that each player controls a different entity or set of entities
- **Pool** — *Os jogadores escolhem entre as 4 classes disponíveis antes do jogo começar.*
    - Pool describes when players are given an array of playable entities to choose from (e.g. characters, nations, class). These have predefined gameplay characteristics and in some cases, players can be f
- **Customisable** — *Cada personagem tem upgrades e equipamentos personalizáveis, que podem ser 
guardados individualmente.*
    - Customisable encompasses all games with upgrades, levelling systems, and others that give players the ability to create and modify their representation throughout the game.
- **Free Collaboration** — *Os jogadores podem escolher espontaneamente quem realiza qual tarefa, podem
minerar, lutar ou iluminar.*
    - In free collaboration, players can take on available tasks as they wish and decide how to contribute.
- **Coupled Collaboration** — *Algumas dessas ações exigem cooperação direta entre classes diferentes 
(ex: torres, iluminação, ou escavação conjunta).*
    - In coupled collaboration, players take on different tasks that somehow intertwine and typically contribute to a shared outcome.
- **Concurrent Collaboration** — *Todas as ações tomadas pelos jogadores acontecem simultaneamente em 
tempo real durante a missão.*
    - In concurrent collaboration, players have to perform gameplay tasks concurrently.
- **Required/Incentivised** — *Comunicação ativa aumenta o nível de coordenação, necessária para 
completar as missões, melhor coordenação devido à comunicação facilita completar a missão com 
sucesso.*
    - When Required/Incentivised, the game challenges require players to communicate (e.g. when there is essential asymmetric information), or incentivised through challenges that are made easier through it
- **Text Chat** — *Os jogadores podem comunicar por mensagens via o text chat na missão, o que reforça a cooperação, e influência de forma positiva a conclusão da missão.*
    - Support for players to communicate using text
- **Pings** — *É possível marcar locais e objetivos com pings para ser mais fácil de identificar o que fazer 
na altura para uma melhor coordenação, o que, por si, influencia a conclusão positiva da missão.*
    - Support for players to communicate by signaling specific things in their view (e.g. aiming at something and signaling)
- **Voice Chat** — *Os jogadores podem comunicar verbalmente uns com os outros para melhor 
colaborar na conclusão da missão com sucesso.*
    - Support for players to communicate using their voice
- **Task** — *Algumas tarefas só podem ser feitas por classes específicas (ex: torre do Enginner).*
    - Task refers to gameplay tasks where at least one player is dependent on another. These can force players to coordinate to be effective and complete them.
- **Spatial** — *Posicionamento correto no mapa é fulcral; exemplo: Driller só pode abrir caminho se 
estiver no local correto.*
    - Spatial, inspired by Reuter et al. 2014, happens when the game forces or incentivises players to be at a certain distance from one another. This can happen in a variety of ways, for example, by creati
- **Scaling Difficulty** — *O jogo ajusta a sua dificuldade (inimigos e desafios) para o número de jogadores 
na missão.*
    - Scaling Difficulty, most games that support a varied player count adapt the difficulty to the number of players, so that players can play with more people if they wish without jeopardising the experie
- **Assistive Actions** — *Existem habilidades como cura, torres e iluminação que ajudam diretamente 
outros jogadores.*
    - Assistive Actions encompass actions that one player performs to the benefit of other/s (i.e. note that the player can have indirect benefits such as reviving others to increase their own chances of su
- **Consumables** — *Existem recursos que podem ser consumidos pelos jogadores, e ao serem 
consumidos, vão existir menos para a equipa, ex: Gunner usar granadas, vai haver menos disponíveis para o resto da equipa.*
    - Consumables refer to items that exist in a limited amount and can be utilised by players to invoke an effect (usually existing as a form of currency, materials, or food) or are not consumed on purpose
- **Unlockables** — *Existem recursos que só após completar dados objetivos são desbloqueados, mas cada 
jogador desbloqueia itens individualmente. Isso significa que o recurso é parcialmente partilhado, 
porque apesar do recurso ser individual, contribui para a conclusão da missão com sucesso.*
    - Unlockables refer to content available in the gameplay but not accessible up until players are able and choose to get access to it. Usually, games limit the number of unlocks a player can acquire. Unl
- **Interactables** — *Alguns recursos exigem a coordenação dos jogadores para uma gestão responsável do 
mesmo (minérios, munição e equipamento).*
    - Interactables refer to virtual objects and non-player characters/entities within the environment that respond to players’ actions. Interactables can be moved, shaped, or activated (e.g., a cannon that
- **Playable characters** — *Cada jogador pré-seleciona uma classe, ao ser selecionada, outros jogadores já 
não a podem selecionar.*
    - Playable characters refer to every entity within the environment whose actions are controlled by player input. As mentioned before, the locus of control may differ from game to game, but usually, each
- **Space** — *Como o espaço é partilhado entre jogadores, pode ser parcialmente considerado um recurso 
estratégico, especialmente para o Driller que precisa de estar em dadas localizações para abrir o 
caminho.*
    - Space is also a resource as it defines the various places in the environment that a player can occupy and interact with. Space is shared when its utilisation is affected or constrained by others' pres
- **Abilities** — *Cada classe tem as suas habilidades únicas e que podem ser complementares com outras 
classes.*
    - Abilities, as described by Harris et al. 2016, are where one player can do things another player cannot. In games where these actions synergise or are complementary, it incentivises collaboration. 
- **Usefulness** — *Todas as habilidades servem e têm utilidade para o sucesso da missão.*
    - Usefulness happens when a certain resource or information (shared among multiple players) is more valuable  to one of those players. It can promote collaboration and coordination to maximise player pe
- **Synergies** — *Combinação das diversas habilidades entre classes aumenta a eficiência da equipa.*
    - Synergies extended from Rocha et. al 2008 allow one entity to assist or change the game actions (e.g. abilities) of another.
- **Complementarity** — *Cada classe completa as outras, o que gera interdependência e reforça 
cooperação.*
    - Complementarity, extended from Rocha et. al rocha2008game, corresponds to when player actions are designed to balance each other’s weaknesses or so that strengths complement each other. It is typicall

---

## It_Takes_Two

- Author: Jorge Guerreiro
- Created: 2023-10-22
- Game mode: coopCampaign
- Analysis level: macro
- Analysis type: pastPlayed
- Framework difficulty (1-5): 2

**Codings:**

- **Party** — *The players have to surpass different phases of the game, so they can advance to the next one. In this 2-player game, they always go together.*
    - Party progression results from the contributions of a group of players that opted to play together, where part of the progress is solely associated with that group (not shared with the rest of the ser
- **Party Creation** — *Players can play in the same machine, but also have the ability to create a party online with a partner.*
    - Party Creation encompasses all games where groups are defined by the players themselves prior to initiating gameplay, typically leveraging friend lists or co-located party creation.
- **Shared** — *The objective of this game is to pass the multiple stages of the game, where players advance together.*
    - Shared goals consist of singular objectives that multiple players pursue and work together to achieve
- **Conflicting** — *In this game there is a few mini-games in which the players can compete with each other, but it does not affect the goal of the game.*
    - Conflicting goals are also observable in some cooperative games, where typically, players compete in certain sections that do not affect the overarching goal of the game. While no semi-cooperative gam
- **Teammates** — *The players are teammates and they need to both cooperate to surpass each stage of the game.*
    - Teammates, where players within a team need to coordinate their actions, abilities, and roles in order to reach a common goal against at least another team. 
- **Competitors** — *In the mini-games, the players compete with each other to see who takes the casual win, that does not have influence in the story of the game.*
    - Competitors, where you may be able to compete with other players, either momentarily or through the whole game session (e.g. team vs team match-based games). 
- **Shared** — *The players interact within the same world.*
    - Shared world views apply to a game when players have access to the same world.
- **Split** — *When players play in the same screen, each player has a viewpoint of its character.*
    - Split typically divides the view by the number of players, with each being associated with a particular section. Players will have a smaller view of the game world as a result, but they will still be 
- **Single** — *Each player takes control of one of the two characters in the game.*
    - Single applies to any game where players have control of one single entity.
- **Pool** — *In this game there are only two characters to be chosen, in the game, they have different abilities to complement each other.*
    - Pool describes when players are given an array of playable entities to choose from (e.g. characters, nations, class). These have predefined gameplay characteristics and in some cases, players can be f
- **Predefined** — *The progress of each player identity is predefined by the storyline.*
    - When progress is strictly linear, with no player choice (e.g., dictated by the storyline, predefined upgrades or progressing through switching entities)
- **Coupled Collaboration** — *The players usually do different tasks that compliment each other's task, so they both can progress.*
    - In coupled collaboration, players take on different tasks that somehow intertwine and typically contribute to a shared outcome.
- **Sequential Collaboration** — *The players have to do tasks in a sequential order, working together, mainly in a coordinated manner to progress.*
    - In sequential collaboration, players have to do tasks in sequential order, with tasks typically assigned to different players.
- **Concurrent Collaboration** — *Players have to do tasks/perform actions in parallel.*
    - In concurrent collaboration, players have to perform gameplay tasks concurrently.
- **Required/Incentivised** — *Players communication is required is this game so that they can coordinate their actions, it is essential to have a good communication to complete the tasks in the game. This communication can be in real life or using other software that provides in real-time communication.*
    - When Required/Incentivised, the game challenges require players to communicate (e.g. when there is essential asymmetric information), or incentivised through challenges that are made easier through it
- **In-Game Movement/Actions** — *It is possible for players to use their characters to shoot something to inform the other player of the task/action they have to perform.*
    - Players can communicate through specific movement or actions (e.g. shooting a weapon on the ground to notify others to pick it up)
- **Task** — *One player is dependent of the other player tasks and actions to advance in the game.*
    - Task refers to gameplay tasks where at least one player is dependent on another. These can force players to coordinate to be effective and complete them.
- **Assistive Actions** — *In this game both have to cooperate so they can both achieve success and surpass the challenges.*
    - Assistive Actions encompass actions that one player performs to the benefit of other/s (i.e. note that the player can have indirect benefits such as reviving others to increase their own chances of su
- **Playable characters** — *The players rely on each other.*
    - Playable characters refer to every entity within the environment whose actions are controlled by player input. As mentioned before, the locus of control may differ from game to game, but usually, each
- **Abilities** — *The player control characters with different abilities, most of which complement with the other.*
    - Abilities, as described by Harris et al. 2016, are where one player can do things another player cannot. In games where these actions synergise or are complementary, it incentivises collaboration. 
- **Complementarity** — *This value of the game is shared, and their abilities complement each other, for example, one shoots nails and the other can support it self in the nails.*
    - Complementarity, extended from Rocha et. al rocha2008game, corresponds to when player actions are designed to balance each other’s weaknesses or so that strengths complement each other. It is typicall

---

