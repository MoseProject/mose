# Auditoria de UX/UI do editor (2026-10-04)

Levantamento feito sobre o frontend em `apps/desktop` depois do redesenho baseado em
`mockup/mose_pixel_mockup`. Método: leitura do código de cada tela (`screens/`, `editor/`,
`TopBar.tsx`), inventário de atalhos (`screens/Editor.tsx`), renderização das telas com o IPC
simulado e checagem das heurísticas de Nielsen, de acessibilidade por teclado e da carga
cognitiva. Não foi feita sessão de uso real com o app nativo; os itens marcados **[verificar]**
dependem disso.

Persona usada (de `PRODUCT.md`): editor de vídeo cortando gravações de gameplay de horas, no
desktop, com teclado e mouse, em sessões longas.

## 1. Resumo

| Área | Nota (0-4) | Principal achado |
|---|---|---|
| Disposição geral | 3 | Estrutura clássica de NLE e fiel ao mockup; faltam alguns agrupamentos |
| Atalhos de teclado | 1 | 14 atalhos, quase nenhum descoberto; faltam J/K/L, I/O, zoom, ferramentas |
| Funcionalidades expostas | 2 | Alvo de loudness e proporção de aspecto existem no backend e não têm UI |
| Modais e diálogos | 2 | 6 modais sem papel `dialog`, sem Esc nem foco preso; 2 `window.confirm` |
| Feedback e estados | 2 | Análise de importação sem progresso visível; erros num banner único |
| Acessibilidade | 2 | Timeline em canvas sem teclado; foco padrão do navegador; sem movimento reduzido |
| Consistência visual | 3 | Paleta e ícones coesos (lucide); tipografia pequena (10-11px) em vários pontos |

Os cinco problemas que mais afetam o uso real, em ordem: **UX-01** não há onde escolher o
loudness-alvo nem a proporção de aspecto (a promessa central do produto); **UX-02** atalhos
globais continuam ativos com um modal aberto; **UX-03** Espaço fica preso pelo atalho global e
quebra a ativação de botões por teclado; **UX-04** a exportação abre direto o seletor de
arquivo, sem diálogo de opções; **UX-05** não existe cheat sheet nem dica de atalho nos botões.

## 2. Disposição dos componentes

### 2.1 Barra superior
- Bom: logo, projeto/sequência, menus, timecode, Exportar à direita. Segue o mockup.
- Menus duplicam ações que já existem na barra da timeline (Clipe, Adicionar). Duplicar é
  aceitável em NLEs (menu + botão), mas hoje os dois nomes diferem ("Gráficos" no menu,
  "Adicionar" na timeline). Unificar os rótulos.
- Há três ícones soltos (desfazer, refazer, salvar) e um de fila de exportação sem rótulo
  visível. Ícone sozinho sem atalho na dica obriga a descobrir por tentativa (ver seção 3).
- O botão de engrenagem e o item Exibir > Ajustes fazem o mesmo; manter só um dos dois.
- Sem projeto aberto, a barra mostra só "Exibir" e a engrenagem. Correto, mas a Home poderia
  ocupar esse espaço com Novo/Abrir/Recentes em vez de uma tela separada.

### 2.2 Trilho de ferramentas (esquerda)
- Seis botões: Selecionar, Rolar, Deslizar origem, Deslizar, Texto e Cortar. Só o texto
  tem atalho mostrado ("Texto (T)").
- Rolar, Deslizar origem e Deslizar são ferramentas avançadas com ícones parecidos (setas
  horizontais). Para quem está começando, parecem a mesma coisa. Agrupar as três num único
  botão com submenu, mantendo Selecionar, Cortar e Texto soltos.
- Falta a ferramenta Mão (rolar a timeline arrastando) e Zoom, presentes no mockup. Hoje o
  zoom só vem por Ctrl+roda ou pelos botões da timeline.

### 2.3 Biblioteca de mídia
- Abas MÍDIA/FAVORITOS/RECENTES, busca e grade/lista ficaram bem.
- O botão "+" esconde Importar, Texto-para-fala, Biblioteca de som e Baixar do YouTube. Importar
  é a ação mais frequente do começo da sessão e hoje precisa de dois cliques. Sugestão: o "+"
  vira botão dividido (clique = Importar, seta = demais fontes), ou o Importar ganha um botão
  próprio ao lado do "+".
- Falta mostrar o tamanho do arquivo e a duração total da seleção.
- Ações no cartão (favoritar, transcrever) só aparecem ao passar o mouse. Para teclado elas
  aparecem com `:focus-within`, mas o cartão em si não é focável (não tem `tabIndex`), então
  nunca recebe foco. **Corrigir**: tornar o cartão focável e permitir Enter para adicionar à
  timeline (hoje só duplo clique).
- Arrastar para a timeline usa eventos de ponteiro; não há alternativa de teclado nem menu de
  contexto ("Adicionar à timeline", "Mostrar no explorador", "Remover da biblioteca"). Remover
  mídia da biblioteca nem aparece como ação. **[verificar]**

### 2.4 Monitor de programa
- Cabeçalho, barra de posição e controles seguem o mockup. O botão play é o único com cor.
- A barra de posição é um `<input type="range">` com passo de 10 ms; em 2h de vídeo cada pixel
  vale mais de 7 s, então arrastar não dá precisão. Combinar com atalhos de passo (setas,
  Shift+setas de 10 quadros) e com a timeline como principal ponto de navegação.
- Faltam: zoom do monitor (50% / Fit / 100%), guias de área segura, indicador de qualidade de
  preview (proxy ou original) e timecode no formato do projeto (hoje mm:ss.cc, não
  HH:MM:SS:FF).
- O monitor tem dupla função de edição (arrastar/redimensionar a camada e clicar com a
  ferramenta Texto). Não há indicação visual de qual modo está ativo além do cursor.

### 2.5 Inspetor
- Abas INSPETOR/EFEITOS/ÁUDIO agrupam as seções do clipe de mídia. Mas "Congelar quadro",
  "Espelhar horizontalmente" e "Limpeza de voz" estão dentro de Velocidade/Ajuste de cor e não
  em Áudio (a limpeza de voz é áudio). **Mover Limpeza de voz para a aba Áudio.**
- Falta uma seção Transformar (posição, escala, rotação, âncora) com campos numéricos; hoje
  posição e escala só existem como keyframes e como arrastar no monitor. É o que o mockup mostra
  de mais importante e o que o editor mais usa.
- Cada seção deveria ter "restaurar padrão" e "adicionar keyframe" (os ícones do mockup).
- O cabeçalho do clipe mostra origem, resolução, fps e duração da fonte, mas não a duração do
  clipe na timeline nem o trecho usado (entrada/saída).
- O inspetor de texto e o de forma usam o mesmo padrão de seções, mas sem abas; o de texto tem
  muitos campos. Agrupar tipografia (fonte, estilo, tamanho, cor, alinhamento) numa seção só.
- Múltipla seleção mostra "Nenhum clipe selecionado". Deveria dizer "2 clipes selecionados" e
  oferecer as ações em lote (mesclar, composto, ganho comum).

### 2.6 Medidor de áudio
- Barras L/R com escala 0 a -60 dB. Falta marcar -14 e -16 LUFS (alvos do produto) e mostrar
  pico retido. Como o diferencial é loudness, o medidor deveria mostrar LUFS momentâneo, não só
  pico. **[verificar]** se o backend expõe LUFS em tempo real.

### 2.7 Timeline
- Cabeçalho em uma linha; funciona, mas fica cheio abaixo de ~1300px de largura. Os três menus
  (Clipe/Analisar/Adicionar) consomem espaço que poderia ir para a régua.
- Cabeçalhos de faixa: nome, olho, cadeado. Faltam mudo/solo para faixas de áudio e o rótulo do
  papel (jogo, microfone, música). Para gameplay com microfone e áudio do jogo, mudo e solo por
  faixa são essenciais.
- O arraste de clipe, trim e roll estão só por mouse. Sem teclado (as setas movem só o cursor).
- Marcadores: lista num pop-over; falta um trilho de marcadores visível na régua com rótulo.
- A faixa de quadros e as ondas dependem de decodificação sob demanda; enquanto carregam o clipe
  fica só com a cor. Falta um estado de "carregando" discreto.
- Cortar, Excluir, Marcadores, Texto e Forma existem em até três lugares (menu, barra da
  timeline, atalho). Não é um defeito, mas as dicas precisam dizer o atalho em todos.

### 2.8 Fila de exportação
- Agora é um pop-over (antes cobria a timeline). Bom. Ainda falta: tempo estimado restante,
  velocidade, caminho de saída clicável ("abrir pasta") e notificação do sistema quando termina
  (a janela pode estar em segundo plano durante exportações longas).

### 2.9 Início, Ajustes e Monitorar pasta
- **Início**: bom como tela de partida, mas vira um beco sem saída quando há projeto aberto
  (o botão "Voltar ao editor" é a única saída).
- **Ajustes**: uma coluna longa sem agrupamento (idioma, loudness, pasta, autosave, 5 caminhos de
  modelo, privacidade, atualização). Dividir em seções (Geral, Exportação, IA e modelos,
  Privacidade, Atualizações) com navegação lateral. Os caminhos de modelo não têm botão
  "Procurar" nem validação (o arquivo existe?). Sem indicação de que salvou (salva a cada
  mudança, silenciosamente). Faltam os atalhos de teclado configuráveis.
- **Monitorar pasta**: ocupa uma tela inteira para uma função de fundo. Candidata a painel
  lateral ou diálogo acionado pelo menu, mantendo o editor visível.

## 3. Atalhos de teclado

### 3.1 Mapa atual (de `screens/Editor.tsx`)
| Atalho | Ação |
|---|---|
| Espaço | Reproduzir/pausar |
| Ctrl+Z / Ctrl+Shift+Z / Ctrl+Y | Desfazer / refazer |
| Ctrl+S (+Shift) | Salvar (salvar como) |
| Ctrl+I | Importar mídia |
| Ctrl+C / Ctrl+X / Ctrl+V | Copiar / recortar / colar clipe |
| Ctrl+Shift+C / V | Copiar / colar formatação |
| S | Dividir no cursor |
| M | Adicionar marcador |
| T | Ferramenta Texto |
| Delete / Backspace (+Shift) | Excluir (com Shift: fechar a lacuna) |
| ← / → (+Shift) | Quadro anterior/próximo (10 quadros) |
| Home | Início |
| Enter / Esc | Concluir / cancelar desenho de forma |

### 3.2 Problemas
- **UX-02** O manipulador é global na janela e só ignora `<input>` e `<select>`. Com um modal
  aberto (Templates, Transcrição, Revisão de silêncio) Delete ainda exclui os clipes atrás, e S
  divide. Também não ignora `<textarea>` nem `contenteditable`. Solução: ignorar atalhos
  quando houver diálogo aberto (contador de diálogos na `uiStore`) e quando o foco estiver em
  qualquer campo editável.
- **UX-03** Espaço sempre chama `preventDefault()`. Um botão focado por Tab não ativa com
  Espaço (e o app começa a tocar). Solução: não interceptar Espaço quando o alvo for um botão,
  link ou controle de formulário.
- Os atalhos não aparecem em nenhum lugar: só "Texto (T)" tem. Nenhum dos menus mostra a
  tecla ao lado do item, e os botões de ícone têm só `title` sem tecla.
- Não há cheat sheet nem atalho para abri-lo (`?` ou Ctrl+/).
- Convenção: S é "dividir" aqui; em Premiere é C (lâmina) e S é "snap". Como o público já edita
  em outras ferramentas, escolher uma convenção e documentar. Recomendação: manter S (já
  existe) e adicionar C como alias.
- Backspace e Delete apagam a seleção sem confirmação; há desfazer, então é aceitável, mas o
  Backspace é fácil de acionar por engano. **[verificar]** se o desfazer cobre todos os casos.

### 3.3 Atalhos que faltam (por frequência de uso em edição de gameplay)
| Prioridade | Atalho sugerido | Ação |
|---|---|---|
| Alta | J / K / L | Retroceder / pausar / avançar (toque repetido acelera) |
| Alta | I / O | Marcar entrada e saída; ligadas a "extrair trecho" |
| Alta | End | Ir ao fim (Home já existe) |
| Alta | + / − (ou Ctrl+= / Ctrl+−) | Zoom da timeline |
| Alta | Shift+Z ou \ | Ajustar a timeline à janela |
| Alta | Ctrl+A / Esc | Selecionar tudo / limpar seleção |
| Alta | ↑ / ↓ | Ir ao ponto de corte anterior/próximo |
| Média | V / C / Y / U | Selecionar / lâmina / deslizar origem / deslizar |
| Média | Ctrl+D | Duplicar clipe |
| Média | Ctrl+E | Exportar |
| Média | Ctrl+K | Paleta de comandos (busca de ações) |
| Média | Shift+M / Alt+M | Marcador anterior / próximo |
| Média | N | Ligar/desligar snap |
| Baixa | F2 | Renomear sequência/clipe |
| Baixa | Ctrl+, | Ajustes |

Uma **paleta de comandos** resolve de uma vez o problema de descobribilidade e substitui a
necessidade de lembrar onde cada ação está; é o item de maior retorno para um app com tantas
funções (IA, transcrição, detecção) espalhadas em menus.

## 4. Zoom da timeline
Corrigido nesta rodada: mínimo caiu de 2 para 0,01 px/s e há botão Ajustar. Pendente:
atalhos, zoom por arrastar na régua e uma barra de navegação (minimap) para vídeos longos que
mostre a janela visível dentro dos 2h45 inteiros.

## 5. Modais, painéis e diálogos

### 5.1 Inventário
| Elemento | Hoje | Observação |
|---|---|---|
| Templates de camada | modal (3 variantes) | sem `role="dialog"`, sem Esc, sem foco preso |
| Transcrição | modal | idem |
| Revisão de silêncio | modal | idem |
| Texto-para-fala, Biblioteca de som, YouTube | modais dentro do menu "+" | idem |
| Revisão de relatório de falha | modal | idem |
| Fila de exportação | pop-over com `role="dialog"` e Esc | único com papel correto |
| Sobrescrever arquivo, descartar alterações | `window.confirm` do sistema | fora do tema |
| Ajustes, Monitorar pasta, Início | telas inteiras | ver abaixo |

### 5.2 Recomendações
1. **Componente `Dialog` único** com `role="dialog"`, `aria-modal`, título rotulado, foco preso,
   Esc para fechar, retorno de foco ao gatilho e registro na `uiStore` (para suspender os
   atalhos globais). Migrar os modais para ele. É a correção que resolve UX-02 e boa parte da
   acessibilidade de uma vez.
2. **Diálogo de exportação** (UX-01). Antes de abrir o seletor de arquivo: nome e pasta,
   proporção (Original, Paisagem, Retrato, Quadrado), loudness-alvo (-14, -16, -23 ou
   personalizado) com o valor medido atual, qualidade (manter o bitrate da fonte, que é o
   diferencial), codificador (hoje em Ajustes) e botões Exportar / Adicionar à fila. Os
   comandos `setExportSettings` e `matchExportLoudness` já existem em `ipc.ts`; só falta a
   interface (`setExportSettings` não é chamado em nenhum componente).
3. **Substituir `window.confirm`** por um diálogo de confirmação do app com os botões nomeados
   pela ação ("Sobrescrever", "Salvar com outro nome", "Descartar") em vez de OK/Cancelar.
4. **Ajustes como diálogo** (Ctrl+,) em vez de tela, para não sair do contexto de edição, com
   as seções da seção 2.9.
5. **Monitorar pasta** como painel/diálogo acionado por Arquivo, com progresso na fila.
6. **Atalhos de teclado** como diálogo (`?`), pesquisável, agrupado por área, com a mesma lista
   usada para gerar as dicas dos botões e dos menus (uma única tabela de atalhos no código).
7. **Propriedades do projeto/sequência** como diálogo (nome, proporção, taxa de quadros,
   loudness-alvo da sequência).
8. Manter como **painéis fixos**: biblioteca, monitor, inspetor, timeline. Manter como
   **pop-over**: menus, marcadores, fila de exportação.
9. Notificações: trocar o banner único de erro por **toasts** empilháveis com ação ("Desfazer",
   "Ver detalhes") e fila, para que um erro de fundo (transcrição, importação) não apague outro.

## 6. Reagrupamento de funcionalidades

| Funcionalidade | Onde está | Proposta |
|---|---|---|
| Importar, TTS, sons, YouTube | menu "+" da biblioteca | Importar visível; demais em "Mais fontes" |
| Detectar capítulos/destaques, Transcrição, Silêncio | menu Analisar da timeline | Menu **Ferramentas de IA** no topo, com progresso na fila de tarefas |
| Rastrear movimento, Reenquadrar, Máscara, Desfoque de privacidade | Inspetor > Ações de IA | Manter no inspetor (são por clipe), com estado "em andamento" |
| Mesclar, Composto, Templates | menu Clipe da timeline | Manter; espelhar no menu de contexto do clipe |
| Adicionar faixa/texto/forma | menu Adicionar da timeline e menus Sequência/Gráficos | Unificar em um só rótulo |
| Alvo de loudness, proporção | só prefs (perfil padrão); comando sem UI | Diálogo de exportação + Propriedades da sequência |
| Encoder GPU | Ajustes | Manter em Ajustes, com resumo no diálogo de exportação |
| Fila de exportação | pop-over na barra superior | Manter; adicionar notificação ao terminar |

Menu de contexto (clique direito) em clipe, faixa, mídia e régua não existe e é o mecanismo
mais usado em NLEs para acessar exatamente essas ações.

## 7. Acessibilidade e teclado
- A timeline é um `<canvas>` sem `tabIndex` nem equivalente textual: usuários de teclado não
  conseguem selecionar, mover ou aparar clipes. Mínimo viável: canvas focável, setas movendo a
  seleção entre clipes, Enter para abrir o inspetor, e uma lista de clipes oculta para leitores
  de tela.
- Sem estilo `:focus-visible` próprio: o anel padrão do navegador some em fundos escuros em
  alguns controles. Definir um anel de 2px na cor de destaque.
- Sem `prefers-reduced-motion`: as transições são curtas (60-140 ms), mas o medidor anima
  continuamente. Respeitar a preferência.
- Tamanhos de texto: 10-11px em metas da biblioteca, valores do inspetor e escala do medidor.
  Subir o mínimo para 11px.
- Alvos de clique: botões de ícone de 24-32px; o trilho tem 38px (bom). Cabeçalhos de faixa e
  alças de trim têm alvos de poucos pixels.
- Ícones sozinhos: todos já têm `aria-label`; falta o atalho no texto da dica.
- Idioma: **[verificar]** se `<html lang>` muda com o idioma da interface.

## 8. Feedback, estados e erros
- Importação: a análise (waveform, loudness, proxy) roda em segundo plano e só se vê
  "analisando…" no texto do cartão. Mostrar uma barra fina no cartão e uma linha na fila de
  tarefas ("Gerando proxy 42%").
- Transcrição e detecções: sem progresso; o usuário não sabe se travou (a de 2h45 leva
  minutos).
- Erros: um único banner no rodapé, substituído pelo seguinte. Ver toasts (seção 5.2).
- Estados vazios: biblioteca e timeline vazias têm texto; falta chamada para ação ("Arraste
  arquivos aqui ou clique em Importar").
- Salvamento: o indicador de alterações não salvas é só um ponto; sem "salvo às HH:MM" e sem
  aviso ao fechar a janela com alterações. **[verificar]** o aviso de fechamento.
- Desfazer: guarda snapshot completo da sequência. Mostrar o nome da ação no histórico
  ("Desfazer: Dividir clipe") ajuda a confiar.

## 9. Backlog priorizado

| ID | Pri | Item | Esforço |
|---|---|---|---|
| UX-01 | P0 | Diálogo de exportação com alvo de loudness e proporção (usa `setExportSettings`) | M |
| UX-02 | P0 | `Dialog` único; suspender atalhos globais com diálogo aberto; ignorar campos editáveis | M |
| UX-03 | P0 | Não interceptar Espaço quando o foco está em botão/controle | S |
| UX-04 | P1 | Atalhos faltantes (J/K/L, I/O, End, zoom, Ctrl+A, Esc, V/C) | M |
| UX-05 | P1 | Cheat sheet (`?`) e atalhos nas dicas e nos menus, de uma tabela única | M |
| UX-06 | P1 | Seção Transformar no inspetor (posição, escala, rotação, âncora) | M |
| UX-07 | P1 | Mudo/solo e papel de áudio nos cabeçalhos das faixas | M |
| UX-08 | P1 | Menu de contexto em clipe, faixa, mídia e régua | M |
| UX-09 | P1 | Progresso visível de importação, transcrição e detecções (fila de tarefas) | M |
| UX-10 | P2 | Paleta de comandos (Ctrl+K) | M |
| UX-11 | P2 | Ajustes como diálogo com seções; Procurar nos caminhos; validação | M |
| UX-12 | P2 | Substituir `window.confirm` por diálogo do app | S |
| UX-13 | P2 | Toasts em vez do banner único | S |
| UX-14 | P2 | Minimap/barra de navegação da timeline para vídeos longos | M |
| UX-15 | P2 | Timeline acessível por teclado (canvas focável, setas entre clipes) | L |
| UX-16 | P2 | Cartões de mídia focáveis; Enter adiciona à timeline; menu de contexto | S |
| UX-17 | P3 | Monitor: zoom 50/Fit/100, guias, timecode HH:MM:SS:FF | M |
| UX-18 | P3 | Medidor com alvos de LUFS e pico retido | M |
| UX-19 | P3 | Agrupar Rolar/Deslizar origem/Deslizar num submenu do trilho | S |
| UX-20 | P3 | `:focus-visible`, `prefers-reduced-motion`, fonte mínima de 11px | S |

Esforço: S até meio dia, M de 1 a 3 dias, L mais de 3 dias.

## 10. O que já está bom (preservar)
- Estrutura de painéis e hierarquia fiel ao padrão dos editores profissionais.
- Fila de exportação fora do caminho; ícones coesos; paleta escura legível.
- Biblioteca com busca, abas e grade/lista; zoom e frames/ondas na timeline reais.
- Menus da barra superior reaproveitam comandos existentes, sem duplicar lógica.
- Cobertura de testes alta no frontend (342 testes), o que permite refatorar com segurança.
