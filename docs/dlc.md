# Como criar uma DLC

Uma DLC é uma expansão comprada uma vez na loja. Todo o conteúdo dela (itens, regras, status, conquistas, telas, animações e sprites) fica num módulo próprio em `src/dlc/<nome>/`, que implementa o trait `Dlc` de `src/dlc/mod.rs`. O jogo base só conhece esses pontos de extensão. A Cadeia (`src/dlc/prison/`) é o exemplo completo.

## Passo a passo

1. **Módulo.** Crie `src/dlc/<nome>/mod.rs` com uma `pub struct` vazia (ex.: `pub struct Prison;`) e declare `pub mod <nome>;` em `src/dlc/mod.rs`.
2. **Registro.** Em `src/dlc/mod.rs`:
   - adicione a variante em `DlcId`;
   - adicione a struct em `ALL` (a ordem é a das abas da loja e das seções de conquistas);
   - se a DLC tiver status, consumíveis armados ou conquistas, adicione a variante correspondente em `DlcStatus`, `DlcBuff` e `DlcAchievement`, e um braço na macro `each_dlc!` (o único `match` que cresce por DLC).
3. **Estado.** Se a DLC guarda dados, adicione um campo com o tipo dela em:
   - `DlcProfile`: salvo com o jogador (contadores, consumíveis armados). Use `#[serde(default)]` para perfis antigos abrirem sem erro;
   - `DlcPublic`: o que todo mundo vê do jogador (vai no protocolo). Preencha em `dlc::public`;
   - `DlcSlot`: timers do servidor enquanto o jogador está online (não é salvo). Preencha em `DlcSlot::new`.
4. **Itens.** Adicione os ids em `ItemId` (`src/items.rs`), num bloco comentado com o nome da DLC, e no braço `unreachable!` de `ItemDef::category`. Devolva os `ItemDef` em `items()`, montados com `permanent(...)` e `consumable(...)`. Um deles é o item que libera a DLC (`unlock_item()`), sem efeitos e com nível máximo 1. Os outros só podem ser comprados por quem tem a DLC; a loja cria a aba sozinha.
5. **Constantes.** Números de regra vão em `src/constants/<nome>.rs`, declarado em `src/constants/mod.rs`.
6. **Sprites.** Coloque `png` + `json` (exportados do Aseprite como strip horizontal) em `assets/dlc/<nome>/` e devolva em `sprite()` com `sprite_asset!("dlc/<nome>/<arquivo>")`. Todo item precisa de sprite; o teste de sprites confere.
7. **Hooks.** Implemente só o que a DLC muda. Os métodos obrigatórios são `id`, `name`, `unlock_item`, `unlocks` e `items`; o resto tem padrão que não faz nada.

## Hooks disponíveis

| Hook | Quando roda | Para quê |
|---|---|---|
| `use_item` | jogador usa um consumível da DLC | arma o efeito ou aplica um status; `None` se o item não é dela |
| `block_mog` | antes de um mog, depois das checagens do jogo | impede o mog com uma mensagem |
| `on_hit` | um mog acertou um jogador | efeitos de acerto (marcar, punir, avisar o servidor) |
| `set_status`, `statuses`, `clear_statuses` | o servidor aplica, lista ou zera status | guardar os timers dos status da DLC no `DlcSlot`; `statuses` acrescenta num `Vec` do chamador |
| `armed` | o cliente monta a faixa de buffs e as conquistas contam consumíveis armados | listar os consumíveis armados |
| `achievements` | lista das conquistas | pares `(DlcAchievement, Def)` num `static`, com `Def::new(Group::Dlc(id), ...)` e `.secret()` |
| `name_tags` | ao lado do nome no cabeçalho e na lista de jogadores | títulos, marcas, contadores |
| `stats_line` | tela de conquistas e estatísticas | uma linha com os números da DLC |
| `rank_lines` | tela de ranks | uma trilha paralela de títulos |
| `help` | página DLCs da ajuda (`?`) | regras da DLC, uma por item da lista |
| `use_fx` | animação de tela cheia ao usar um consumível | cor, duração, tremida e a função que pinta cada célula |
| `forehead` | ao desenhar um avatar | número e cor escritos na testa |

Os hooks de tela (`name_tags`, `stats_line`, `rank_lines`, `forehead`) só rodam para quem comprou a DLC (`dlc::owned_by`): quem não tem a expansão não vê nada dela nos outros jogadores. Partes de avatar exclusivas da DLC ficam em `src/avatar.rs` com `dlc: Some(id)`.

Status implementam `StatusKind` (rótulo, efeito, se é ruim, multiplicadores de poder e defesa) e consumíveis armados implementam `BuffKind` (rótulo, efeito, cor do selo, item). Para mexer no mundo, use a API do `World` (`event`, `announce`, `afflict`, `add_cooldown`) e do `Slot` (`set_status`, `mark_dirty`) em vez de editar os campos direto. O jogo base aplica os multiplicadores e encurta status ruins com a Terapia sem saber de qual DLC eles vêm.

## Testes

Crie `tests/<nome>.rs` cobrindo pelo menos: itens bloqueados sem a DLC, o item de desbloqueio comprado uma vez, e cada regra do servidor. Mudou o protocolo (novo campo em `DlcPublic` ou novo status)? Suba `PROTOCOL_VERSION`.
