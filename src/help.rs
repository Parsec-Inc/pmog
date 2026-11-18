//! Help manual, built from the live constants and catalog so numbers never drift.

use ratatui::prelude::*;

use crate::constants::*;
use crate::game::{Achievement, CATALOG, Game, Rank};
use crate::utils::format_mp;

pub const PAGES: [&str; 8] = ["Básico", "Combate", "Economia", "Ranks", "Loja", "Duelos", "Social", "Glossário"];

/// Lines of one help page.
pub fn page(index: usize, g: &Game) -> Vec<Line<'static>> {
    match index {
        0 => basics(),
        1 => combat(g),
        2 => economy(g),
        3 => ranks(g),
        4 => shop(),
        5 => duels(),
        6 => social(),
        _ => glossary(),
    }
}

fn title(text: &str) -> Line<'static> {
    Line::styled(text.to_uppercase(), Style::new().fg(Color::Yellow).bold())
}

fn text(t: impl Into<String>) -> Line<'static> {
    Line::raw(t.into())
}

fn bullet(t: impl Into<String>) -> Line<'static> {
    Line::from(vec![Span::styled("  • ", Style::new().fg(Color::Magenta)), Span::raw(t.into())])
}

fn key(k: &str, what: &str) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!(" {k:<8}"), Style::new().fg(Color::Cyan).bold()),
        Span::raw(what.to_string()),
    ])
}

fn term(name: &str, meaning: &str) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("  {name}: "), Style::new().fg(Color::LightMagenta).bold()),
        Span::raw(meaning.to_string()),
    ])
}

fn gap() -> Line<'static> {
    Line::raw("")
}

use crate::utils::decimal as n;

fn pct(v: f64) -> String {
    format!("{}%", n(v * 100.0))
}

fn duels() -> Vec<Line<'static>> {
    use crate::duel::Minigame;
    let mut lines = vec![
        title("Duelo direto"),
        text(format!("Fora do botão MOGAR, dá para chamar alguém para um duelo de verdade: {DUEL_ROUNDS} rodadas de minigames sorteados.")),
        gap(),
        key("D", "desafia o jogador selecionado (escolha a aposta e aperte Enter; ↓ troca Aleatório por Escolher)"),
        key("F", &format!("entra na fila: junta você com alguém até 1 rank de distância (depois de {} s esperando, com qualquer um)", QUEUE_RELAX.as_secs())),
        key("A / R", "aceita ou recusa um desafio recebido"),
        key("X", "cancela o desafio que você mandou"),
        key("Esc", "dentro do duelo, desiste (perde a aposta)"),
        gap(),
        title("Escolha e rodadas"),
        bullet("Por padrão os minigames são sorteados. Para escolher, desça para a linha Minigames no seletor de aposta e marque Escolher. Na fila, só vale se os dois marcarem."),
        bullet(format!(
            "Escolhendo, antes de começar cada um pega {} minigames, alternando (quem começa é sorteado). Use as setas e Enter; o Aleatório sorteia um que ninguém escolheu. Acabou o tempo de {} s, o servidor sorteia por você.",
            DUEL_ROUNDS / 2,
            DRAFT_PICK_TIME.as_secs()
        )),
        bullet(format!("As {DUEL_ROUNDS} rodadas seguem a ordem do sorteio ou das escolhas. O duelo acaba antes se alguém abrir uma vantagem que não dá mais para tirar.")),
        bullet(format!("Empatou depois de {DUEL_ROUNDS}? Morte súbita com um minigame sorteado que ainda não saiu, até alguém vencer uma rodada (no máximo {DUEL_MAX_ROUNDS} rodadas no total).")),
        gap(),
        title("Aposta"),
        bullet("A aposta é uma parte do MP do mais pobre dos dois: amistoso (0), 10%, 25%, 50% ou tudo."),
        bullet("As duas apostas ficam guardadas no servidor durante o duelo. Quem vence leva as duas (é MP de outro jogador, então não conta para o rank)."),
        bullet(format!("Empate depois de {DUEL_MAX_ROUNDS} rodadas, ou servidor desligado no meio: cada um recebe a sua de volta.")),
        bullet("Sair do jogo ou cair no meio do duelo conta como desistência."),
        bullet(format!("Um desafio expira em {} s. Na fila, vale a menor das duas apostas.", INVITE_TTL.as_secs())),
        bullet("Duelo amistoso, ou desistência antes de terminar a primeira rodada, não conta no placar nem para conquistas."),
        gap(),
        title("Minigames"),
    ];
    for game in Minigame::ALL {
        lines.push(term(game.name(), game.how_to()));
    }
    lines.extend([
        gap(),
        title("Lag não conta"),
        text("O jogo mede o atraso da sua rede e conta o momento em que você apertou, não quando chegou no servidor. Quem joga no Wi-Fi ruim disputa de igual para igual com quem hospeda. O servidor confere esse horário e não aceita trapaça."),
        gap(),
        title("Nome com estilo"),
        text("Seu nome muda de cara conforme o rank: estático até MTN, brilho pulsando no HTN, gradiente no Chadlite, reflexo dourado no Chad, fogo no Gigachad, arco-íris com coroa no Ascendido e um efeito próprio em cada rank acima (veja todos com P)."),
    ]);
    lines
}

fn basics() -> Vec<Line<'static>> {
    vec![
        title("Objetivo"),
        text("Juntar Mogging Points (MP) moggando quem está com o PMOG aberto na sua rede, comprar itens e subir de rank até Transcendente."),
        gap(),
        title("A tela"),
        bullet("Topo: seu nome, rank, MP, streak, seus números de combate e a barra até o próximo rank."),
        bullet("Esquerda: quem está na rede. O ▶ marca o alvo selecionado."),
        bullet("Centro: o botão MOGAR, o medidor de timing, o cooldown e os avisos grandes."),
        bullet("Direita: o feed com tudo o que aconteceu, o mais novo em cima."),
        bullet("A borda do topo pisca em vermelho quando alguém te mogga."),
        gap(),
        title("Controles"),
        key("ESPAÇO", "moggar o alvo selecionado (Enter também funciona)"),
        key("↑ ↓", "trocar de alvo"),
        key("M", "mewar: ganhar MP na hora"),
        key("L", "abrir a loja (↑ ↓ escolhe, Enter compra)"),
        key("1 a 9, 0", "usar um consumível"),
        key("C", "conquistas e estatísticas"),
        key("P", "todos os ranks, com o visual de cada nome"),
        key("S", "painel do servidor"),
        key("D / F", "duelo direto ou fila de duelo (veja a página Duelos)"),
        key("?", "esta ajuda (← → troca de aba, ↑ ↓ rola)"),
        key("Esc", "fechar a janela aberta"),
        key("Q", "salvar e sair"),
        gap(),
        title("Servidores"),
        bullet("O progresso de cada jogador fica guardado no servidor em que ele joga, não no computador dele."),
        bullet("Na tela inicial aparecem os servidores encontrados na rede. Enter conecta no selecionado."),
        bullet("Servidor que não aparece (outra rede, VPN sem broadcast): aperte I na tela inicial e digite o IP, ou IP:porta. O painel S de quem hospeda mostra todos os endereços dele, inclusive o da VPN."),
        bullet("Mundos: cada mundo é um save separado, com seus próprios jogadores e progresso. \"Criar mundo novo\" pede um nome, liga o servidor aqui e já te conecta. Quem estiver na mesma rede vê o mundo na lista com esse nome."),
        bullet("Os mundos salvos aparecem na tela inicial: Enter hospeda, R renomeia e A apaga (vai para a lixeira do PMOG). Dá para hospedar vários ao mesmo tempo, cada um numa porta; P para o mundo selecionado."),
        bullet("Atualização: se alguém hospedar uma versão mais nova, ou se sair uma versão nova no GitHub, o jogo pergunta se você quer atualizar. Ele só aceita versões assinadas pelo dono do jogo, confere tudo antes de trocar o executável e reabre sozinho. Se disser não, aperte U na tela inicial depois."),
        bullet("O servidor decide tudo: duelos, compras e MP. Ninguém consegue inventar números."),
        bullet("Cada mundo é um banco SQLite no computador de quem hospeda (~/.local/share/pmog/worlds/). Os apagados ficam em ~/.local/share/pmog/lixeira/."),
        bullet("Se quem hospeda fechar o jogo, os mundos desligam para todos, mas o progresso fica salvo."),
        bullet("Tecla S, durante o jogo: painel do servidor (quem está online, endereço para o celular, desconectar, parar servidor)."),
        bullet("Firewall: libere a porta UDP 47777 e as portas TCP 47790 a 47799 (e 47800 a 47809 para o celular). Se o firewall estiver no caminho, a tela inicial avisa e mostra o comando para liberar."),
        gap(),
        title("Celular"),
        bullet("Cada PMOG aberto pode ser jogado de um celular, um por vez, na mesma rede. Aperte C (tela inicial ou painel S) para criar a senha; aí aparecem o endereço (http://ip:porta) e o QR code."),
        bullet("No primeiro acesso o celular pede a senha. Depois o navegador fica lembrado, até a senha mudar."),
        bullet("E troca o modo: espelho (PC e celular controlam a mesma sessão) ou assume (o PC só assiste; R no PC retoma). X desconecta o celular."),
        bullet("Sair pelo celular só solta o celular: o PMOG continua aberto no PC."),
        gap(),
        title("Seu computador"),
        text("Cada computador é identificado pelo UUID do hardware, guardado só como hash. Trocar de nome, rede ou IP não muda nada: você continua com o mesmo progresso em cada servidor. Na primeira vez o jogo pede a senha de administrador para ler esse UUID."),
        gap(),
        title("NPC de Treino"),
        text("Alvo que está sempre disponível. Serve para ganhar MP sozinho e treinar o timing. Mogs nele não contam streak."),
    ]
}

fn combat(g: &Game) -> Vec<Line<'static>> {
    let s = g.player.stats();
    let timing_max = TIMING_FLOOR + TIMING_SPAN * PERFECT_THRESHOLD;
    vec![
        title("Um mog, passo a passo"),
        bullet("1. Escolha o alvo e aperte ESPAÇO."),
        bullet("2. Seu poder = Poder x timing x streak (x2 com Foto no Ângulo)."),
        bullet(format!(
            "3. O alvo sorteia a defesa dele: Defesa x um valor entre {} e {}.",
            n(DEFENSE_ROLL.start),
            n(DEFENSE_ROLL.end)
        )),
        bullet("4. Se o seu poder for maior, você mogga. Se não, o mog falha: você perde um pouco de MP e o streak."),
        bullet(format!(
            "Crítico: em qualquer mog há uma chance de {} x Poder / (Poder + Defesa do alvo), nunca menos de {}. O crítico atravessa qualquer defesa e leva x{} de MP, então até quem está bem abaixo tem uma chance.",
            pct(CRIT_CHANCE_SCALE),
            pct(CRIT_MIN_CHANCE),
            n(CRIT_TAKE_MULT)
        )),
        bullet(format!(
            "Mentalidade Underdog soma chance de crítico a cada vez que a defesa do alvo dobra sobre o seu poder, até {} no total. Ego Ferido garante o crítico no próximo mog em quem te moggou por último.",
            pct(CRIT_MAX_CHANCE)
        )),
        bullet(format!(
            "Rank pesa: mogar alguém de rank maior rende até x{} mais, e quem tem rank maior perde até x{} mais quando falha contra um rank menor.",
            n(1.0 + RANK_WEIGHT_SPAN),
            n(1.0 + RANK_WEIGHT_SPAN)
        )),
        bullet(format!(
            "Status de luta: quem leva crítico fica ABALADO ({} s, defesa -{}). Quando um rank menor vence um maior, o menor ganha EGO PROIBIDO e o maior fica com PERDEU AURA por {} s: poder +/- {}, {} ou {} para 1, 2 ou 3+ ranks de diferença. Perdeu Aura não volta antes de {} s depois de acabar.",
            ABALADO_DURATION.as_secs(),
            pct(ABALADO_DEF_CUT),
            UPSET_DURATION.as_secs(),
            pct(RANK_GAP_STRENGTH[0]),
            pct(RANK_GAP_STRENGTH[1]),
            pct(RANK_GAP_STRENGTH[2]),
            PERDEU_AURA_GRACE.as_secs()
        )),
        bullet(format!("Sessão de Terapia encurta Abalado e Perdeu Aura, até {} a menos.", pct(DEBUFF_CUT_CAP))),
        bullet("Ao lado do alvo aparece sua chance de vencer com timing BOM e com PERFECT, já com os status de luta dos dois. Escudo e Espelho do alvo não entram na conta, porque ficam escondidos."),
        gap(),
        title("Timing"),
        text("Um cursor vai e volta no medidor abaixo do botão. O momento em que você aperta faz duas coisas: define seu poder (a chance de vencer) e multiplica o MP que você ganha se vencer."),
        bullet(format!(
            "Vermelho, RUIM (até {} da barra): MP x{}.",
            pct(TIMING_OK_FROM),
            n(REWARD_BAD)
        )),
        bullet(format!(
            "Amarelo, OK ({} a {}): MP x{}.",
            pct(TIMING_OK_FROM),
            pct(TIMING_GOOD_FROM),
            n(REWARD_OK)
        )),
        bullet(format!(
            "Azul, BOM ({} até a zona verde): MP x{}.",
            pct(TIMING_GOOD_FROM),
            n(REWARD_GOOD)
        )),
        bullet(format!(
            "Verde, PERFECT (a partir de {}): MP x{} e poder x{}, que vence qualquer defesa do mesmo nível.",
            pct(PERFECT_THRESHOLD),
            n(REWARD_PERFECT),
            n(PERFECT_MULT)
        )),
        bullet(format!(
            "Fora da zona verde, o poder vai de x{} (ponta esquerda) até x{} (logo antes da zona verde).",
            n(TIMING_FLOOR),
            n(timing_max)
        )),
        bullet("Óculos Escuros aumentam a zona verde. Meditação Sigma deixa o cursor mais lento."),
        gap(),
        title("Streak"),
        bullet(format!(
            "Cada vitória seguida soma +{} de poder (máximo {} vitórias).",
            pct(BASE_STREAK_BONUS),
            STREAK_CAP
        )),
        bullet(format!(
            "Em compensação, o cursor fica {} mais rápido a cada vitória.",
            pct(METER_STREAK_ACCEL)
        )),
        bullet("Perder um mog zera o streak, e ser moggado também. Mogs no NPC não contam."),
        bullet(format!(
            "Quem quebra o streak de alguém leva +{} do prêmio por vitória do streak quebrado (o líder vira alvo valioso).",
            pct(STREAK_BOUNTY)
        )),
        gap(),
        title("Resultados de um mog seu"),
        bullet(format!(
            "Vitória contra jogador: você tira {} + {} do MP dele (máximo {}), multiplicado pelo timing. Ele perde exatamente o que você ganha.",
            n(STAKE_FLAT),
            pct(STAKE_RATIO),
            format_mp(STAKE_CAP)
        )),
        bullet(format!(
            "Bater em quem está {PUNCH_DOWN_GAP} ranks abaixo rende só {} e não conta streak. A partir de {PUNCH_DOWN_ZERO_GAP} ranks abaixo, não rende nada.",
            pct(PUNCH_DOWN_FACTOR)
        )),
        bullet("Vitória contra o NPC de treino: o streak que você trouxe de mogs em jogadores multiplica o prêmio, mas não cresce."),
        bullet(format!(
            "Derrota (reverse mog): você perde {} do seu MP (mínimo {}) e o streak zera.",
            pct(LOSS_PENALTY_RATIO),
            n(LOSS_PENALTY_FLAT)
        )),
        bullet("Alvo imune: a lista mostra quanto falta para ele voltar, e tentar antes não gasta seu cooldown."),
        bullet("Alvo com Escudo Sigma: nada acontece, mas o cooldown é gasto."),
        gap(),
        title("Quando te moggam"),
        bullet("Você sorteia sua defesa contra o poder do atacante."),
        bullet(format!(
            "Perdeu: o atacante leva parte do seu MP e você fica {} s imune.",
            IMMUNITY.as_secs()
        )),
        bullet(format!(
            "Moggado de novo em seguida: a imunidade triplica a cada vez, até {} s. Depois de {} s em paz, volta ao normal.",
            IMMUNITY_MAX.as_secs(),
            IMMUNITY_RESET.as_secs()
        )),
        bullet(format!("Segurou: você ganha {} do que o atacante perdeu (itens de contra-ataque aumentam, até {}).", pct(COUNTER_RATIO), pct(MAX_COUNTER_RATIO))),
        bullet("Escudo Sigma bloqueia o ataque inteiro. Espelho de Bolso triplica sua defesa uma vez."),
        gap(),
        title("Cooldown"),
        bullet(format!(
            "Depois de cada mog você espera {} s (mínimo {} s com Trilha Phonk).",
            n(BASE_COOLDOWN_SECS),
            n(MIN_COOLDOWN_SECS)
        )),
        bullet("Energético zera o cooldown. Pré-Treino corta pela metade por 30 s."),
        gap(),
        title("NPC de Treino"),
        bullet(format!(
            "Defesa sorteada entre {} e {} do seu poder.",
            pct(NPC_DEFENSE_ROLL.start),
            pct(NPC_DEFENSE_ROLL.end)
        )),
        bullet(format!(
            "Recompensa: {} + {} do seu poder, multiplicada pelo timing e pelo streak.",
            n(NPC_REWARD_FLAT),
            pct(NPC_REWARD_RATIO)
        )),
        gap(),
        title("Seus números agora"),
        bullet(format!("Poder {}   Defesa {}", format_mp(s.attack), format_mp(s.defense))),
        bullet(format!(
            "Zona PERFECT a partir de {}   Cursor {} da velocidade normal",
            pct(s.perfect_threshold),
            pct(s.meter_factor)
        )),
        bullet(format!(
            "Cooldown {} s   Bônus por streak {}   Streak atual {} (x{})",
            n(s.cooldown),
            pct(s.streak_bonus),
            g.streak,
            n(g.streak_mult(&s))
        )),
        bullet(format!(
            "Bônus nas vitórias +{}   Proteção nas perdas -{}   Imunidade {} s",
            pct(s.win_bonus),
            pct(s.loss_shield),
            n(s.immunity.as_secs_f64())
        )),
    ]
}

fn economy(g: &Game) -> Vec<Line<'static>> {
    let s = g.player.stats();
    vec![
        title("Mogging Points (MP)"),
        text("MP é a moeda do jogo e também o placar. Existem dois números no topo da tela:"),
        bullet("MP: o que você tem agora para gastar."),
        bullet("Total: tudo o que você já ganhou na vida. É o Total que define o rank, então gastar na loja nunca te derruba."),
        gap(),
        title("Como ganhar"),
        bullet(format!(
            "Mewar (M): {} MP por aperto, no máximo {} apertos por segundo.",
            n(1.0),
            MEW_CAP_PER_SEC
        )),
        bullet("Passivo: alguns itens geram MP por segundo enquanto o jogo está aberto."),
        bullet(format!(
            "O passivo conta no máximo {} s por vez, então pausar o jogo ou suspender o PC não rende MP.",
            n(MAX_TICK_SECS)
        )),
        bullet("Vencer mogs contra gente da rede e contra o NPC."),
        bullet("Segurar mogs que fazem contra você."),
        gap(),
        title("Como perder"),
        bullet("Reverse mog: quando o seu ataque falha."),
        bullet("Ser moggado: o atacante leva parte do seu MP."),
        bullet(format!(
            "Sessão de Terapia reduz essas perdas (no máximo {}).",
            pct(MAX_LOSS_SHIELD)
        )),
        bullet("Comprar itens na loja."),
        gap(),
        title("Seu ritmo agora"),
        bullet(format!(
            "{} MP por mewada, até {} por segundo",
            n(s.mew_gain),
            s.mew_cap
        )),
        bullet(format!("{} MP/s de passivo", n(s.passive))),
        bullet(format!("+{} de MP em cada vitória", pct(s.win_bonus))),
        bullet(format!("{} do que o atacante perde, quando você segura um mog", pct(s.counter_ratio))),
        bullet(format!("+{} de crítico a cada vez que a defesa do alvo dobra sobre seu poder", pct(s.underdog))),
        bullet(format!("status ruins duram {} menos", pct(s.resilience))),
    ]
}

fn ranks(g: &Game) -> Vec<Line<'static>> {
    let current = g.player.rank();
    let mut lines = vec![
        title("Ranks"),
        text("O rank sobe conforme o MP Total. Cada rank libera itens novos na loja."),
        gap(),
    ];
    for rank in Rank::ALL {
        let unlocks: Vec<&str> = CATALOG.iter().filter(|d| d.min_rank == rank).map(|d| d.name).collect();
        let marker = if rank == current { "▶ " } else { "  " };
        let style = if rank <= current { Style::new().bold() } else { Style::new().fg(Color::DarkGray) };
        lines.push(Line::from(vec![
            Span::styled(format!("{marker}{:<14}", rank.label()), style),
            Span::styled(format!("{:>8} MP  ", format_mp(rank.threshold())), Style::new().fg(Color::Cyan)),
            Span::raw(unlocks.join(", ")),
        ]));
    }
    lines.push(gap());
    lines.push(title("Conquistas"));
    lines.push(text("Marcos que ficam salvos no seu perfil (tecla C para ver as suas e o progresso). Algumas são secretas."));
    for a in Achievement::ALL.into_iter().filter(|a| !a.secret()) {
        lines.push(term(a.name(), a.desc()));
    }
    lines
}

fn shop() -> Vec<Line<'static>> {
    let mut lines = vec![
        title("Como a loja funciona"),
        bullet("Abra com L. ← → trocam de categoria (Rosto, Físico, Drip e Status, Aura, Brainrot e 67, Consumíveis), ↑ ↓ escolhem o item e Enter compra."),
        bullet("O número ao lado da categoria diz quantos itens dela você já pode comprar."),
        bullet("O painel da direita mostra o que o item faz e quanto seus números mudam com a compra."),
        bullet(format!(
            "Itens permanentes têm níveis. Cada nível soma o efeito de novo e custa {}x o anterior ({}x na Aura Infinita).",
            n(COST_GROWTH),
            n(AURA_COST_GROWTH)
        )),
        bullet("Consumíveis têm preço fixo, ficam no inventário e são usados com as teclas 1 a 9 e 0."),
        bullet("Escudo, Foto no Ângulo, Espelho, Fita de Mandíbula e Combo 6-7 ficam preparados até serem gastos, mesmo se você fechar o jogo."),
    ];
    for category in crate::items::Category::ALL {
        lines.push(gap());
        lines.push(title(category.label()));
        for def in category.items().map(|i| &CATALOG[i]) {
            let text = if def.consumable() {
                format!("{}. Rank {}.", def.summary(), def.min_rank.label())
            } else {
                format!("{} por nível. Rank {}, até nível {}.", def.summary(), def.min_rank.label(), def.max_level)
            };
            lines.push(term(def.name, &text));
        }
    }
    lines
}

fn glossary() -> Vec<Line<'static>> {
    vec![
        title("Gírias do jogo"),
        term("Mog / moggar", "ofuscar alguém, ser claramente superior. Vem de AMOG, \"alpha male of the group\"."),
        term("Looksmaxxing", "a busca por maximizar a própria aparência. É o tema dos itens da loja."),
        term("Mewing", "técnica de manter a língua no céu da boca para definir o maxilar. No jogo, é o clicker."),
        term("Frame mog", "moggar pelo porte físico."),
        term("Height mog", "moggar pela altura. Daí a Palmilha de 7 cm."),
        term("Wealth mog", "moggar pela riqueza. Daí o Jatinho Particular."),
        term("Aura", "presença, carisma. A Aura Infinita multiplica tudo."),
        term("Sigma", "o meme do lobo solitário que não se abala. Daí o Escudo e a Meditação."),
        term("Phonk", "gênero musical das edições de vídeo de \"gigachad\"."),
        term("Gua sha", "pedra de jade usada em massagem facial."),
        term("Softmaxxing / hardmaxxing", "melhorar a aparência com hábitos (pele, sono, treino) ou com métodos extremos. No jogo, só o soft."),
        term("Frame mog / brutalmente frame-moggado", "ficar ao lado de alguém com o porte muito maior. Daí a Ombreira de Frame Mog."),
        term("Jestermaxxing", "usar o humor como arma de mog. Também é um item."),
        term("Sleepmaxxing / proteinmaxxing", "o sufixo -maxxing vale para tudo: dormir, comer proteína, tomar sol."),
        term("Aura farming", "fazer algo de propósito para parecer descolado: a dança na proa do barco, encostar na porta, câmera lenta."),
        term("+1000 / -1000 de aura", "placar imaginário de quanto uma atitude te deixou mais (ou menos) descolado."),
        term("6-7", "o \"six seven\" sem significado nenhum, com as mãos subindo e descendo como uma balança. Daí o Gesto do 6-7, a altura de 6'7\" e o Combo 6-7."),
        term("Brainrot", "humor absurdo da internet, os bichos do brainrot italiano feitos por IA. Na loja: Tralalero Tralala, Chimpanzini Bananini, Tung Tung Tung Sahur e Bombardiro Crocodilo."),
        term("Capoeira proibida", "meme dos edits de golpes de capoeira absurdos com phonk e caveira: \"eu sabendo capoeira proibida ☠️\". É o item de poder e contra-ataque da aba Brainrot e 67."),
        gap(),
        title("Ranks"),
        term("Sub-5", "abaixo da nota 5, o ponto de partida."),
        term("LTN / MTN / HTN", "low, mid e high-tier normie: normal de nível baixo, médio e alto."),
        term("Chadlite", "quase um Chad."),
        term("Chad", "o arquétipo do cara confiante que mogga sem esforço."),
        term("Gigachad", "o Chad levado ao extremo, o meme do rosto esculpido."),
        term("Ascendido", "quem completou a jornada do looksmaxxing."),
        term("Lorde da Aura", "quem farmou aura até não caber mais."),
        gap(),
        title("Termos do jogo"),
        term("MP", "Mogging Points, a moeda e o placar."),
        term("Streak", "vitórias seguidas. Aumenta o poder e a velocidade do cursor."),
        term("PERFECT", "apertar na zona verde do medidor."),
        term("Reverse mog", "seu ataque falhou e o alvo te moggou de volta."),
        term("Imune", "quem acabou de ser moggado não pode ser moggado de novo por alguns segundos; o tempo restante aparece ao lado do nome."),
        term("NPC", "o boneco de treino local."),
    ]
}

fn social() -> Vec<Line<'static>> {
    let mut lines = vec![title("Chat (T)")];
    lines.extend(
        [
            "A lista da esquerda tem Geral, o canal do clã, grupos e conversas privadas.",
            "Geral é visto por todo mundo online. Privadas e grupos ficam salvos, e você recebe o que perdeu quando voltar.",
            "Enter envia, Tab ou ↑↓ troca de conversa, PgUp/PgDn rola o histórico, Esc fecha.",
            "Ctrl+N: nova mensagem privada. Ctrl+G: cria um grupo. Ctrl+A: adiciona alguém ao grupo. Ctrl+L: sai do grupo.",
            "O número em T (n) no rodapé são as mensagens não lidas. Há limite de tamanho e de velocidade contra spam.",
        ]
        .map(bullet),
    );
    lines.extend([gap(), title("Clãs (G)")]);
    lines.extend(
        [
            "Clã tem até 20 jogadores e uma tag, como [GIGA], que aparece ao lado do nome de todos os membros.",
            "Criar é grátis: um nome (3 a 24 letras) e uma tag única (2 a 5 letras ou números).",
            "O líder convida jogadores online sem clã com I. Quem recebe aceita com A ou recusa com R.",
            "Membros do mesmo clã não se mogam e não apostam entre si. Duelo amistoso pode, e a fila nunca junta colegas.",
            "O líder pode expulsar (K), passar a liderança (P) ou desfazer o clã (D). Qualquer membro sai com S.",
            "Se o líder sair, a liderança vai para o membro mais antigo. Quando o último sai, o clã some.",
            "A aba Ranking ordena os clãs pelo MP total acumulado dos membros.",
        ]
        .map(bullet),
    );
    lines.extend([gap(), title("Notificações do sistema")]);
    lines.extend(
        [
            "Com a janela do jogo fora de foco, o PMOG avisa quando você é moggado, recebe desafio, termina um duelo, sobe de rank ou ganha conquista.",
            "Também avisa mensagens privadas, de grupo e do clã, convites de clã e quando a conexão cai.",
            "Muitos avisos seguidos viram um só (por exemplo, \"Você foi moggado 10 vezes\").",
            "Se o terminal não informa o foco, vale como fora de foco depois de 30 s sem apertar teclas.",
            "Liga e desliga no painel do servidor (S), tecla B. A escolha fica salva.",
        ]
        .map(bullet),
    );
    lines
}
