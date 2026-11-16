# PMOG (parses mogging)

Jogo de terminal em Rust para a rede local. Uma pessoa hospeda um servidor pela própria interface, as outras entram pela lista de servidores, e todo mundo se mogga.

## Rodar

```bash
cargo run --release                  # abre a tela inicial
cargo run --release -- --name Nome   # já entra com esse nome
./scripts/bots.sh 4                  # 4 bots entram no primeiro servidor achado na rede
cargo test --release                 # regras, duelos, servidor real + clientes TCP, stress com 10 bots, TUI
```

Na tela inicial: `↑↓` escolhe, `Enter` entra num servidor ou hospeda um, `N` troca o nome, `P` para o servidor que você hospeda, `Q` sai.

## Simulação e monitoramento

```bash
cargo run --release --example sim -- 8 120      # 8 bots, 120 min de jogo em segundos: invariantes, economia, duelos
cargo run --release --example latency -- 300 25 10  # justiça com lag: bot local x bot com 25 ms (±10) de ida, com e sem compensação
cargo build --release && cargo run --release --example live -- 10 180 /tmp/live.db   # servidor real + 10 bots, falhas injetadas, auditoria do banco
```

## Distribuir

`./scripts/build-dist.sh` gera `dist/pmog-linux`, um executável estático que roda em qualquer Linux x86_64 sem instalar nada.

## Atualização automática

Quem hospeda com a versão nova entrega o executável pela rede para quem entra com a versão velha. O jogo pergunta antes, baixa, confere e reabre sozinho (U na tela inicial pergunta de novo).

- **Só builds assinadas são aceitas.** Cada build é assinada com Ed25519 pela chave privada em `~/.config/pmog-release/release.key`, que nunca sai deste computador. A chave pública (`release-pubkey.txt`) vai embutida em todo executável na hora de compilar. A assinatura cobre plataforma, número da build, tamanho e SHA-256: um host malicioso não consegue trocar os bytes, mudar a plataforma, fazer downgrade nem assinar nada próprio.
- A verificação acontece com o arquivo ainda na memória; nada é gravado em disco se falhar. O download tem limite de 64 MB e de tempo.
- Builds de desenvolvimento (`cargo run`) nunca se atualizam nem oferecem atualização.
- Quem atualizou guarda o `.sig` ao lado do executável e passa a oferecer a mesma build quando hospeda.
- Se a chave privada vazar, qualquer um pode assinar builds: guarde um backup offline e não copie ela para outro lugar. Se ela se perder, gere outra (`cargo run --example release -- keygen` depois de mover a antiga) e mande o executável novo aos amigos uma vez, à mão.

Para publicar: `./scripts/build-dist.sh` compila, assina e deixa tudo em `dist/`. Jogue/hospede rodando `dist/pmog-linux` (o host oferece as duas plataformas que estiverem na mesma pasta). `cargo run --example release -- check dist` mostra o que será oferecido.

## Como funciona

- **Servidor autoritativo**: quem hospeda roda o servidor dentro do próprio jogo. Ele decide duelos, compras, cooldowns e o medidor de timing. O cliente só manda ações.
- **Progresso no servidor**: fica num SQLite em `~/.local/share/pmog/server.db` de quem hospeda (jogadores, histórico de duelos e compras). Trocar de servidor = progresso separado.
- **Identidade = computador**: cada PC é identificado por um hash do UUID de hardware (`/sys/class/dmi/id/product_uuid` + serial da placa). Ler esses arquivos exige root no Linux, então o jogo pede a senha uma vez via `pkexec` e guarda só o hash em `~/.local/share/pmog/machine-id`. Trocar de nome mantém o progresso.
- **Mesma máquina, duas janelas**: as duas têm o mesmo id; a mais nova assume e a outra é desconectada. Para testar com vários jogadores num PC só, use os bots (cada bot tem id próprio).
- **Nome**: salvo em `~/.local/share/pmog/config.json`.
- **Vindo do Mogger**: na primeira vez, a pasta antiga `~/.local/share/mogger` é movida para `~/.local/share/pmog`, com progresso, servidor e identidade do computador.

## Duelos PvP

- `D` desafia o jogador selecionado, `F` entra na fila (pareia até 1 rank de distância; depois de 10 s, qualquer um). Quem recebe aceita com `A` ou recusa com `R`.
- Aposta: amistoso, 10%, 25%, 50% ou tudo do MP do mais pobre. As duas apostas ficam presas no servidor e o vencedor leva tudo; empate ou servidor desligado devolve. Sair no meio conta como desistência.
- Melhor de 3, com minigames sorteados: **Encarada** (reflexo), **Rotina de Skincare** (memória com setas), **Corrida de Mewing** (alternar ← → rápido), **Ângulo Perfeito** (parar o cursor no alvo), **Batalha de Digitação** (digitar uma frase de gíria looksmaxxing: vale palavras por minuto x precisão², com bônus para quem termina primeiro), **Treino no Ritmo** (ESPAÇO em cada batida do metrônomo), **Olhar de Caçador** (apertar só nos hunter eyes ◣_◢, nunca na cara de cachorrinho ◕‿◕) e **Conta dos Macros** (resolver somas e subtrações de gramas digitando o número).
- Antes de cada rodada há 5 s mostrando o nome e a explicação do minigame, e 3,5 s de pausa com o resultado entre uma rodada e outra.
- Amistoso e desistência antes do fim da primeira rodada não contam no placar nem para conquistas.
- Lag não conta: o servidor mede o ping de cada um (Ping/Pong a cada 1 s), o cliente informa quando apertou e o servidor aceita esse horário só dentro da janela que o ping permite. Na Encarada, o AGORA aparece pelo relógio de cada cliente e o servidor espera 200 ms por uma reação mais rápida ainda a caminho.
- Histórico em `matches` no banco do servidor; vitórias e derrotas aparecem no cabeçalho.

## Jogo limpo entre jogadores

- Mog entre jogadores só troca MP de mão: o atacante ganha exatamente o que o defensor perde (o timing e a Ring Light aumentam quanto você tira; o streak só vale contra o NPC).
- Bater em quem está 2 ranks abaixo rende 25% e não conta streak; 3 ou mais ranks abaixo não rende nada.
- Quem é moggado várias vezes seguidas fica imune por mais tempo (5 s, 15 s, 45 s, até 60 s); depois de 60 s sem apanhar, volta a 5 s. O tempo que falta aparece ao lado do nome na lista e no alvo, e tentar mogar alguém imune não gasta seu cooldown.

## Social

- **Chat (`T`)**: Geral, mensagens privadas, grupos e canal do clã. O histórico fica no banco do servidor; quem estava offline recebe as privadas e de grupo ao entrar, com contagem de não lidas.
- **Clãs (`G`)**: nome e tag única ao lado do nome do jogador, convite/aceite, expulsar, passar liderança, ranking por MP total. Membros do mesmo clã não se mogam nem apostam entre si (amistoso liberado, fila não junta colegas).
- **Notificações do sistema**: só com o terminal fora de foco (ou 30 s sem teclas, se o terminal não informa o foco), agrupadas em rajadas, via `notify-send` ou aviso do próprio terminal. Liga/desliga com `B` no painel do servidor.

## Nome por rank

Até o MTN, o nome tem cor fixa. No HTN ele pulsa; no Chadlite vira um gradiente que corre; no Chad tem reflexo dourado com ✦; no Gigachad pega fogo com ✸; e no Ascendido fica arco-íris com ♛.

## Rede e firewall

- Descoberta: o servidor anuncia em UDP `47777` (multicast `239.77.77.77` + broadcast).
- Jogo: TCP na primeira porta livre entre `47790` e `47799`.

No computador que hospeda (Fedora/firewalld):

```bash
sudo firewall-cmd --add-port=47790-47799/tcp --add-port=47777/udp
```

Quem só entra num servidor precisa receber UDP `47777` para ver a lista.

## Controles no jogo

`ESPAÇO` mogar · `↑↓` alvo · `M` mewar · `L` loja · `C` conquistas · `S` servidor · `D` duelar · `F` fila · `1-8` consumíveis · `?` ajuda · `Q` sair

No painel do servidor (`S`): `D` desconecta, `P` para o servidor (se você hospeda), `N` troca o nome.
