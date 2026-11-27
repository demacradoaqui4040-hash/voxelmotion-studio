# VoxelMotion Studio — Especificación Maestra de Rediseño (`v1.0 · Especificación Aprobada`)

> **Estado del documento:** `v1.0 Aprobada` (`Piezas #1–#4` cerradas; implementación `v2` autorizada).  
> **Regla de ejecución:** Toda decisión `DEC-01`..`DEC-17` rige como contrato de producto para la implementación en Rust, WGSL y Slint.

---

## 1. Propósito y Contrato de Calidad

El rediseño `v2` busca transformar VoxelMotion Studio en una estación de animación 3D nativa diseñada desde primeros principios alrededor del flujo real del creador (elegir arma/ítem, animar poses y keyframes, validar visualmente en 3D y exportar al juego) sin sobrecarga visual ni ruido de paneles heredados de software DCC tradicional.

### Principios no negociables

1. **Primero especificación, después código:** cada cambio de layout, widget, shader o dirección artística se acuerda y valida aquí antes de tocar la implementación.
2. **Ayudar sin ensuciar:** todo elemento permanente en pantalla debe justificar su costo visual frente al flujo principal de trabajo. Lo que no aporta orientación directa ni acción frecuente vive en popovers bajo demanda o atajos.
3. **Cero recolors disfrazados de estilo:** ninguna dirección artística puede limitarse a cambiar colores de acento. Cada preset curado debe alterar **geometría** (radios), **materialidad** (opacidad, borde, contraste), **tipografía**, **forma de controles** (switches, tabs, sliders) y **escenario/iluminación 3D**.
4. **Cero abreviaturas crípticas ni mezcla SVG + ASCII:** quedan prohibidas etiquetas truncadas o ambiguas (`Mand.`, `Orig:`, `Est:`, `Mej`, `Cp`, `8H`, `<KF`, `KF>`, `-1f`, `+1f`).
5. **Dominio real del lienzo 3D:** el viewport 3D debe ocupar al menos **62%–68% del área útil** con paneles de trabajo activos, y más de **88%** al colapsar paneles (`Modo Enfoque`).
6. **Presupuesto de rendimiento:** mantener `0.00%` de CPU en reposo (`t >= 12s`) y garantizar `>= 120 FPS` estables incluso durante la transición crítica **órbita con ratón → soltar botón → zoom con rueda**.

---

## 2. Registro de Decisiones Bloqueadas (Pieza por Pieza)

Las decisiones marcadas aquí son definitivas salvo reapertura explícita en una iteración posterior.

### Pieza #1 — Barra Superior *(Cerrada)*

| ID | Decisión aprobada | Justificación y comportamiento exacto |
| :--- | :--- | :--- |
| **`DEC-01`** | **Función de la Barra Superior (Espacio único sin pestañas)** | La barra superior cumple únicamente 3 roles legítimos: control de ventana OS, orientación del clip activo (`Attack 1`) y acciones globales del proyecto (`Archivo ▾` y ajustes del estudio). Se eliminan nombre/publicidad de la app (`"VoxelMotion"`), selectores rápidos de tema, funciones locales 3D y pestañas de navegación redundantes (`Animaciones \| Biblioteca \| Proyecto \| Ajustes`). |
| **`DEC-02`** | **Título centrado solo con Clip Activo (sin arma repetida ni semáforo fijo)** | El centro muestra únicamente el nombre del clip activo (`Attack 1 • ▾`, con punto discreto solo si hay cambios pendientes de guardado, clic para cambiar/crear clip y doble clic para renombrar inline). El nombre del arma no se repite en la barra superior. La incompatibilidad arma/clip avisa contextualmente sobre el visor 3D solo cuando bloquea la reproducción. |
| **`DEC-03`** | **Sin botones `Deshacer / Rehacer` en Barra Superior ni en Inspector** | Eliminados de ambos lugares por baja ergonomía real en PC (desplazamiento largo vs. `Ctrl+Z`) y por consumir espacio prioritario en la esquina superior izquierda. Permanecen accesibles vía atajos (`Ctrl+Z`, `Ctrl+Shift+Z`, `Ctrl+Y`), menú de aplicación y paleta `Ctrl+K`. |
| **`DEC-04`** | **Menú explícito `Archivo ▾` en esquina superior izquierda** | Reemplaza al ícono decorativo `[✦]` por texto directo **`Archivo ▾`** para ofrecer máxima claridad en Windows sin añadir fricción de descubrimiento. Agrupa las acciones del ciclo de vida del proyecto: `Nueva escena en blanco`, `Importar ítems (.png/.json)…`, `Cargar presets demo`, `Exportar proyecto / RELL Seas…`, `Exportar tira PNG`, `Deshacer / Rehacer` (respaldo descubrible de `DEC-03`) y `Atajos de teclado`. |
| **`DEC-05`** | **Sin toggles de paneles estilo IDE (`[◧] [⬓] [◨]`) en Barra Superior** | Los controles de abrir/cerrar paneles laterales pertenecen a IDEs (chat, terminal, explorador), no a una estación de animación 3D donde los paneles forman parte fija del flujo de trabajo. La acción de despejar y expandir el visor 3D (`Enfoque / Expandir [⛶]`, atajo `Tab`) se traslada directamente a la esquina del propio visor 3D. |
| **`DEC-06`** | **Sin botón de lupa (`[🔍]`) en Barra Superior (`Ctrl+K` silencioso)** | Se elimina el ícono visual de búsqueda en la barra superior para evitar tres lupas compitiendo en pantalla (Biblioteca, Clips y techo). La paleta de búsqueda rápida se conserva con cero costo visual mediante el atajo `Ctrl+K` y una entrada explícita dentro de `Archivo ▾`. |
| **`DEC-07`** | **Sin botón `Exportar` en Barra Superior (`Archivo ▾` + `Ctrl+E`)** | Se elimina el botón de exportación de la barra superior para evitar ruido cromático constante y clics accidentales junto a los controles de ventana. Exportar (`Proyecto / RELL Seas F3+T`, configuración de formato/carpeta y `Tira PNG`) vive de forma explícita dentro de `Archivo ▾` y con atajo directo `Ctrl+E`. |
| **`DEC-08`** | **Engranaje discreto `[⚙]` en esquina superior derecha** | Mantiene acceso inmediato y universal en Windows a `Ajustes` (`Ctrl+,`) mediante trazo monocromo fino separado por un micro-divisor vertical (`1px × 14px`) de los controles de ventana. |
| **`DEC-09`** | **Diseño visual `Silent Chrome` (`36px`) para Barra Superior** | Barra fundida con `bg-shell` sin línea horizontal divisoria (gana `+16px` de altura para el 3D; borde exterior de ventana de `1px` en modo no maximizado). Controles en reposo sin caja ni borde (`Ghost-Until-Hover` en `70–80ms`). Controles de ventana OS (`— □ ✕`) trazados con `Path` vectoriales exactos (`10×10px`, trazo `1.15px`) en áreas táctiles `44×36px` con esquina infinita en `✕` al maximizar. Adaptación estructural real según Dirección Artística (`Pro Neutral` fundida vs. línea de chasis `1px` + `JetBrains Mono` en estilos industriales/técnicos). |
| **`DEC-10`** | **Estándar Global de Craft Visual (Logos, Iconografía, Tipografía y Grilla)** | (1) **Familia única de íconos:** prohibido mezclar SVG manuales con glifos Unicode/ASCII; adopción integral de una sola familia profesional open-source (`MIT`: `Lucide` o `Phosphor`) sobre grilla exacta `24×24px` y peso consistente (`1.5px`). (2) **Isotipo propio:** diseño geométrico propio de 2–3 planos legibles desde `16×16px` a `512×512px` para archivo `.svg` e `.ico` de Windows en reemplazo de `✦`. (3) **Escala tipográfica estricta de 4 niveles:** `13px` títulos, `12px` controles/listas, `11px Mono` lecturas numéricas/frames tabulares y `10px` metadatos. (4) **Grilla de `4px` y 3 tonos calibrados:** espaciados múltiplos de `4px`, compensación óptica en íconos asimétricos (`Play +1px`) y separación tonal limpia en vez de cajas anidadas. |

### Pieza #2 — Visor 3D y Arquitectura de Superficies *(Cerrada)*

| ID | Decisión aprobada | Justificación y comportamiento exacto |
| :--- | :--- | :--- |
| **`DEC-11`** | **Riel de herramientas reducido a 3 transformaciones reales + toggle de gizmo** | Se eliminan los modos redundantes `Seleccionar` y `Cámara` (ya que el clic puntual selecciona huesos y arrastrar en vacío/botón derecho orbita siempre). El riel contiene exclusivamente `Mover`, `Rotar` y `Escalar` con íconos vectoriales de la familia única (`DEC-10`), más un toggle limpio para ocultar/mostrar el gizmo al inspeccionar la pose. |
| **`DEC-12`** | **Arquitectura de Superficie Continua con divisores `1px` (Cero carditis de cajas anidadas)** | Se eliminan los huecos negros de `10px`, las `8 PanelCard` y las `13 InsetGroup` anidadas (`52` declaraciones actuales de `border-radius` en `ui/app.slint`). La ventana pasa a ser una sola superficie continua separada únicamente por líneas limpias de `1px` y aire estructural (`12px`), recuperando espacio real para el 3D edge-to-edge. Solo conservan elevación propia los controles flotantes sobre el 3D y popovers/modales. |
| **`DEC-13`** | **Navegador izquierdo dedicado a `Clips` + Selector monocromo de Arma (`[Arma ▾] < >`) con `Fijar` en desplegable** | Se elimina definitivamente el doble dock inferior (`Pistas + Clips`) y el panel fijo de 139 armas. El panel izquierdo (`244px`) muestra arriba una fila compacta monocroma de `36px` con el ítem equipado (`[ícono monocromo · Katana Kozuki ▾] < >`) y abajo la grilla vertical de **Clips** con scroll natural. Al abrir el desplegable del arma se accede a búsqueda, filtro de familia, curación (`Aprobar / Mejorar / No`), `+ Importar` y un toggle monocromo **`Fijar`** que permite anclar temporalmente la lista de armas en la columna izquierda al curar en masa (`0 px` de costo visual en reposo; reutiliza el mismo componente). Abajo existe exclusivamente un único **Timeline (`Pistas`)**. |
| **`DEC-14`** | **Transporte vectorial mínimo en la cabecera del Timeline y base del 3D 100% libre** | Se elimina la barra de transporte flotante sobre los pies del personaje, la pastilla duplicada `BrazoDerecho · Rotar` y el título duplicado sobre el 3D. El transporte se depura a los controles vectoriales esenciales (`KF Ant.`, `-1f`, `Play/Pausa`, `+1f`, `KF Sig.`, lectura `04/29` y velocidad `1.0x`), eliminando `|<`, `>|` y `KF 1/18` redundantes, y se ubica en la cabecera del Timeline (causa junto a efecto). Solo flota suavemente sobre el 3D cuando el Timeline se oculta en modo expandido (`Tab` / `[⛶]`). |
| **`DEC-15`** | **Esquina superior derecha 3D mínima (`[Cámara ▾] [Vista ▾] [⛶]`, sin tríada XYZ por defecto)** | Se reemplazan los 7 botones textuales y la cajita fija de la tríada XYZ por tres controles monocromos limpios (`Cámara ▾`, `Vista ▾`, `[⛶]`). `Vista ▾` consolida todas las capas visuales del visor (`Solo Arma`, `Cebolla`, `Estela`, `Sombras`, `Entorno`), descargando `Escena` del Inspector derecho. La ausencia de la tríada queda marcada para **validar en uso real** al probar el primer prototipo (evaluando entonces si hace falta o no agregar una tríada opcional). |

### Pieza #3 — Inspector Derecho *(Cerrada)*

| ID | Decisión aprobada | Justificación y comportamiento exacto |
| :--- | :--- | :--- |
| **`DEC-16`** | **Inspector de 2 pestañas (`Pose` \| `Reglas y VFX`) con lista directa de 8 huesos integrada** | Se elimina la pestaña dedicada `Huesos` para no cortar el ritmo cambiando de pestaña constantemente. En `Pose` (sin `Escena`, sin `Deshacer/Rehacer` y sin cajas `InsetGroup`), los `~698px` verticales disponibles permiten mostrar todo en una sola vista limpia: (1) lista directa de los 8 huesos con visibilidad `👁` y `Ver todos`, (2) cabecera del hueso activo con acciones vectoriales (`Copiar`, `Pegar`, `Espejo`, `Reset`) + `Posición / Rotación / Escala`, y (3) `Animación` (`Bucle`, `Duración`, `Curva`), con scroll suave automático si la ventana es más baja. La segunda pestaña `Reglas y VFX` agrupa objetos permitidos, impacto y eventos VFX. |

### Pieza #4 — Timeline (`Pistas`) *(Cerrada)*

| ID | Decisión aprobada | Justificación y comportamiento exacto |
| :--- | :--- | :--- |
| **`DEC-17`** | **Timeline único (`156px`) sin siglas crípticas con Transporte integrado** | Barra superior (`32px`) con `Pistas` + `● Auto` (`Auto-Keyframe`) a la izquierda, el Transporte vectorial de `DEC-14` centrado sobre la regla y 4 acciones limpias de keyframe a la derecha (`+ Keyframe`, `Duplicar`, `Marcar Impacto ⚡`, `Borrar`). Debajo, regla de frames (`22px`) con selector `4 huesos ▾` / `Todos (8)` en la columna izquierda (`244px`) y `102px` de pistas que alojan las 4 pistas principales completas sin recortes ni abreviaturas (`8H`/`Cp`/`AutoKF`), con scroll vertical suave al expandir a 8 huesos. |

### Matriz de Priorización Pareto 80/20 *(Impacto UX estimado)*

Distribución de esfuerzo por impacto real percibido al usar la aplicación (porcentajes estimados de prioridad de diseño para evitar invertir tiempo en pulido cosmético antes de resolver los cuellos de botella centrales):

| Prioridad | Palanca de diseño / ingeniería | Peso estimado | Qué cambia concretamente para el usuario |
| :--- | :--- | :--- | :--- |
| **1** | **Cámara y respuesta 3D sin lag (`órbita → zoom`)** | **25%** | Coalescing de redibujo por frame, órbita sin captura accidental de huesos, lotes de 1–2 miniaturas solo en reposo real y guardado de disco fuera del hilo UI. |
| **2** | **Limpieza estructural del espacio (`>= 68%` 3D)** | **22%** | Superficie continua `1px` (`DEC-12`), eliminar el doble dock inferior (`Pistas + Clips`) y quitar las 10 cajas fijas de categorías en el panel izquierdo. |
| **3** | **Presentación 3D de estudio y gizmo limpio** | **18%** | Iluminación 3 puntos, sombra de contacto real al suelo y gizmo fino que no atraviesa el rostro del personaje. |
| **4** | **Controles directos sin abreviaturas ni fricción** | **15%** | Familia única de íconos (`DEC-10`), transporte vectorial sin texto críptico, popovers `Cámara ▾` / `Vista ▾`, playhead legible y renombrado inline de clips. |
| **5** *(Base 80% lista)* | **Direcciones Artísticas reales (`DesignSpec`) + Isotipo** | **20%** | Cambio real de geometría, materialidad, tipografía, controles y escenario 3D por preset una vez que el núcleo (`1–4`, `~80%`) ya responde impecable. |

---

## 2.B Autopsia Crítica del Diseño Actual (`v1`)

Auditoría basada en la inspección directa del código fuente (`ui/app.slint`, `ui/theme.slint`, `ui/widgets.slint`, `src/ui_bridge.rs`, `src/camera.rs`, `src/renderer3d.rs`, `src/shaders.wgsl`) y la captura real `exports/final_studio.png`:

| # | Problema detectado | Evidencia confirmada en el proyecto actual | Impacto en calidad percibida |
| :--- | :--- | :--- | :--- |
| **1** | **Saturación de 4 capas en panel izquierdo** | `Biblioteca` (`242px`) apila buscador, fila `Todas`, estado, matriz fija de 10 familias (`Espadas`, `Katanas`, `Mand.`, `Hachas`, etc.), lista scrollable e `InsetGroup` inferior en solo ~540px verticales. | La lista real de ítems queda comprimida a ~7 filas visibles y obliga a truncar nombres de categorías (`Mand.`). |
| **2** | **Doble dock inferior (~39% de altura robada)** | En `1440×890`, `Pistas` (`130px`) + `Clips` (`128px`) + espaciados consumen casi 280px verticales; la cabecera de `Clips` amontona 17 controles en una sola línea. | Comprime el escenario 3D verticalmente y fragmenta la atención entre dos tiras horizontales competidoras. |
| **3** | **Abreviaturas crípticas y botones ASCII** | `<KF`, `-1f`, `+1f`, `KF>`, `8H`, `Cp`, `Orig: Todo`, `Est: Todo`, `Compat.`, `Mej`. | Rompe la coherencia visual frente a iconos vectoriales y dificulta entender qué hace cada acción. |
| **4** | **Cápsula superior textual + sombras dinámicas en FemtoVG** | La barra flotante superior mezcla contexto (`Arma · Clip`) y 7 botones de texto (`Personaje`, `3/4`, `Frente`, `Lado`, `Espalda`, `Enfocar`, `Centrar`), mientras 4 overlays sobre el visor usan `drop-shadow-blur`. | Recarga la parte alta del personaje y añade costo de rasterización vectorial sobre cada frame activo del viewport. |
| **5** | **Presentación 3D plana y gizmo intrusivo** | Fondo casi negro, grilla oscura poco calibrada y `gizmo_ring` blanco grueso (`make_ring_mesh(6.4, 0.14, ...)`) atravesando el rostro del personaje. | Da aspecto de visor técnico crudo en vez de estudio de animación profesional. |
| **6** | **Temas actuales = solo cambio de color** | `theme-idx` en `ui/theme.slint` y `src/shaders.wgsl` modifica hex de fondos, acento y grilla; radios, tipografía, densidad, switches y geometría son idénticos entre `0`, `1` y `2`. | Cambiar de tema no ofrece experiencias estéticas realmente distintas. |
| **7** | **Lag en `órbita (click izq.) → zoom`** | Confirmado en `src/ui_bridge.rs`: (a) `on_vp_pointer_move` llama `sync_triad_only()` (6 mutaciones Slint + reteselado de 3 `Path`) + `request_immediate_redraw()` en cada paquete crudo del ratón; (b) al soltar, `BeforeRendering` ejecuta `generate_gallery_thumbs_batch(20)` de forma síncrona si hay miniaturas pendientes; (c) con herramienta `Rotar` activa, arrastrar sobre hueso muta pose y dispara guardado de `profile.local.json` (`41.8 KB`) en el hilo UI. *La proporción exacta en milisegundos de cada causa requiere un micro-benchmark específico (`--bench-orbit-zoom`).* | Al pasar rápido de orbitar a hacer zoom con rueda, se percibe tirón y respuesta irregular del viewport. |

---

## 3. Principios REALES que Apple practica en 2025–2026 (No Marketing)

Lecciones verificadas del despliegue real de **macOS 26 Tahoe ("Liquid Glass")** y de las aplicaciones profesionales actuales de Apple (**Final Cut Pro 11**, **Logic Pro** y **Reality Composer Pro**):

1. **La transparencia es enemiga de la precisión:** durante las betas de macOS 26 Tahoe, aplicar vidrio translúcido con refracción en barras laterales e inspectores destruyó el contraste de texto, introdujo ruido de fondo en timelines y generó recortes por radios `12px+` en tablas densas. Apple respondió agregando el modo `Tinted` en Tahoe 26.1+ y **enjaulando/desactivando el vidrio translúcido dentro de paneles de trabajo de Final Cut Pro 11 y Logic Pro**. El vidrio real queda confinado a superficies efímeras y flotantes (`Spotlight`, popovers, menús contextuales, toasts), nunca a inspectores ni timelines.
2. **Gris neutro calibrado (`#1c1c1e`–`#2c2c2e`), nunca `#000000` ni `#ffffff` en apps Pro:** tanto Final Cut Pro 11 como Reality Composer Pro fuerzan una apariencia oscura neutra calibrada (incluso si el sistema operativo está en Light Mode) para proteger la fidelidad cromática del contenido, la lectura de efectos brillantes y la ergonomía visual en sesiones largas.
3. **Revelación progresiva real (*Progressive Disclosure*):** en vez de exhibir 17 botones simultáneos sobre un timeline o viewport, FCP 11 y Xcode esconden opciones secundarias en popovers limpios (`Vista ▾`, `Cámara ▾`, `Filtros ▾`) y permiten mostrar u ocultar cada panel maestro (`Biblioteca`, `Clips`, `Inspector`) con botones dedicados en la barra superior y atajos de teclado.
4. **Menos cajas anidadas:** las apps Pro de Apple separan secciones mediante cambio tonal sutil, divisiones limpias de `1px` y grilla estricta `4pt / 8pt`, evitando apilar 4 bordes concéntricos (`Panel → InsetGroup → Input → Pill`).
5. **Reconocimiento sobre memoria (*Recognition > Recall*):** los iconos sin etiqueta solo funcionan para símbolos universales de medios o sistema (`Play`, `Pause`, `Step`, `Search`, `Eye`, `+`, `Trash`, `Undo/Redo`). Conceptos específicos de dominio (`Cebolla`, `Estela`, `Auto-Key`, `Solo Arma`) deben llevar texto corto o vivir en un popover etiquetado.
6. **Manipulación directa y latencia `<16ms` antes que adorno:** scrubbers numéricos arrastrables, atajos de una sola tecla y respuesta instantánea de cámara tienen prioridad absoluta sobre blurs o sombras decorativas encima del render 3D.

---

## 4. Red-Team de `v0.1`: Contraargumento por Contraargumento

Evaluación sin complacencia de cada propuesta que figuraba en `v0.1`, contrastada contra lo que Apple realmente implementa en 2026:

| Propuesta en `v0.1` | Contraargumento real Apple 2026 | Veredicto para `v0.2` |
| :--- | :--- | :--- |
| **Compositor libre de 480 mezclas + selector de tema en la barra superior** | Apple jamás expone un configurador de 480 skins ni un selector de tema en la barra principal de Final Cut Pro o Reality Composer Pro: destruye la memoria muscular, diluye la identidad del software y multiplica regresiones de contraste. | **RECHAZADO.** Se elimina el selector de tema de la barra superior. El compositor pasa fuera del flujo de trabajo principal y **toda selección estética vive exclusivamente en `Ajustes`**. En lugar de 480 mezclas arbitrarias, se ofrecen **Presets Curados completos** auditados visualmente, con **1 modo estándar por defecto (`Pro Studio Neutral`)**. |
| **`Daylight Ceramic` con ciclorama 3D blanco (`#f7f8fa`)** | Un escenario 3D blanco vuelve ilegibles las estelas VFX cian/doradas, la línea de impacto y los ejes claros; por eso FCP 11 y Reality Composer Pro mantienen el lienzo neutro oscuro aunque la interfaz circundante sea clara. | **CORREGIDO.** Si existe un preset claro (`Daylight Ceramic`), su chrome exterior es claro (`#f2f3f5` / `#ffffff`), pero el **visor 3D mantiene por defecto un gris estudio medio calibrado (`#2e3138`)** con el ciclorama blanco únicamente como opción manual de fotografía para vistas sin VFX. |
| **Pestañas excluyentes `[ Ítems \| Clips ]` en la izquierda** | En VoxelMotion el flujo central cruza **Arma × Clip** (seleccionar arma y auditar compatibilidad o curar varios clips seguidos). Separarlos en pestañas excluyentes duplica clics en cada cruce. | **MEJORADO.** Adoptar el modelo real de **FCP 11 / Xcode**: paneles independientes conmutables desde la barra superior (`[Biblioteca]`, `[Clips]`, `[Inspector]`). **Al editar keyframes**, el cajón `Clips` se colapsa con 1 clic (`>= 68%` para el 3D); **al curar armas × clips**, ambos pueden convivir visibles sin cambiar de pestaña. |
| **"100% iconografía sin texto" en controles** | Poner iconos abstractos sin texto para `Cebolla`, `Estela`, `Auto-Key` u `8 Huesos` viola *Recognition > Recall* y obliga al usuario a adivinar o memorizar tooltips. | **CORREGIDO.** Transporte de reproducción (`Inicio`, `KF Ant.`, `Frame -1`, `Play/Pause`, `Frame +1`, `KF Sig.`, `Fin`) usa **iconos vectoriales universales**; los toggles de dominio (`Cebolla`, `Estela`, `Solo Arma`, `Sombras`, `Grilla`) se consolidan en un único botón popover etiquetado **`Vista ▾`** con texto completo. |
| **Reemplazar presets de cámara por clic en tríada `48×48px`** | Una tríada de `48×48px` deja objetivos de clic de `~6–8px` por eje, violando la Ley de Fitts y causando errores constantes al intentar cambiar a Frente o Lado. | **CORREGIDO.** Tríada interactiva de tamaño cómodo (`68×68px` con nodos clicables de `16px` en `X/Y/Z`) **más** un popover compacto **`Cámara ▾`** (`3/4`, `Frente`, `Lado`, `Espalda`, `Enfocar`, `Centrar`), manteniendo la barra limpia sin perder precisión. |
| **Barra superior `46px` con 12 elementos** | Demasiada densidad horizontal sin respiro visual; compite con la barra de navegación. | **SIMPLIFICADO** al estándar **Xcode / FCP 11** en 3 islas claras (`44px`): Izquierda (Toggles `[Biblioteca \| Clips \| Inspector]` + contexto breve), Centro (`Animaciones \| Biblioteca \| Proyecto \| Ajustes`), Derecha (`Buscar Ctrl+K` compacto + `Exportar` primario + controles de ventana). |
| **Radios `16px` y `Liquid Glass` en todos los paneles** | Es exactamente el error que Apple corrigió tras las betas de macOS Tahoe 26 en sus apps Pro por recortar contenido denso y bajar contraste. | **CORREGIDO.** Paneles de trabajo con **superficies opacas neutras calibradas** y radios sobrios (`8px–10px` en estándar Pro); efecto `Tinted Glass` translúcido reservado exclusivamente a **popovers, Spotlight (`Ctrl+K`) y toasts**. |

---

## 5. Referentes Técnicos Open-Source Auditables (Solo Ingeniería y Tokens)

> **Criterio de `v0.3`:** Se descartan como inspiración visual las cabinas sobrecargadas de suites DCC tradicionales (`Cinema 4D`, `Maya`, `Houdini`) por priorizar densidad heredada sobre claridad de trabajo. El diseño visual de interfaz se construye desde primeros principios centrados en el usuario; únicamente se consultan proyectos Open-Source de arquitectura compatible para buenas prácticas de ingeniería y tokens:

- **Rerun (`rerun-io/rerun`, `MIT/Apache-2.0`, Rust + `wgpu`):** visor espacial 3D en tiempo real, paneles limpios sin bordes ruidosos y convenciones tipográficas claras (`DESIGN.md`).
- **Zed (`zed-industries/zed`, `crates/theme` & `crates/ui`, Rust + GPUI):** motor data-driven de Design Tokens completamente separados de la lógica de cada widget.
- **Fyrox (`FyroxEngine/Fyrox`, `MIT`, Rust):** separación técnica entre gizmos 3D, raycast de selección, viewport embebido y pistas de animación.
- **Graphite (`GraphiteEditor/Graphite`, `Apache-2.0`, Rust):** riel de herramientas minimalista y paneles de propiedades con mínima fricción visual.
- **Blockbench & Blender (`GPL`, consulta estricta de formatos/atajos):** compatibilidad Bedrock/GeckoLib, atajos estándar de cámara y comportamiento de órbita/zoom alrededor del pivote.

---

## 6. Arquitectura Espacial `v2` (Superficie Continua según `DEC-01`..`DEC-15`)

### Distribución unificada de 4 zonas (`>= 68%` área útil en 3D)

```text
+---------------------------------------------------------------------------------------------------+
| [Archivo ▾]                              Attack 1 • ▾                               [⚙] — □ ✕ |  36px
+--------------------+-------------------------------------------------------+----------------------+
| [⚔ Katana ▾] <  >  | +---+                       [Cámara ▾][Vista ▾][⛶]    |  Pose            Reglas y VFX |
|--------------------| |Mov|                                                 |-------------------------------|
| Clips      [+] [▾] | |Rot|                                                 | Esqueleto           Ver todos |
| [Buscar clip...  ] | |Esc|           ESCENARIO 3D PROTAGONISTA             |  * BrazoDerecho            👁 |
| [Todos filt.  ▾]   | |---|             (>= 68% del lienzo)                 |  * BrazoIzquierdo          👁 |
|                    | |👁 |                                                 |  * Espada / Cuerpo / Piernas  |
| +------+  +------+ | +---+                                                 |-------------------------------|
| |Att. 1|  |Att. 2| |            Personaje + Arma + Iluminación 3-pt        | BrazoDerecho  [Cop][Peg][⇄][↺]|
| +------+  +------+ |            + Sombra de contacto + Gizmo fino          |  Posición    [ X ][ Y ][ Z ]  |
| |Parry |  |Block | |                                                       |  Rotación    [ X ][ Y ][ Z ]  |
| +------+  +------+ |                                                       |  Escala      [ X ][ Y ][ Z ]  |
| |Idle  |  |Run   | |                                                       |-------------------------------|
| +------+  +------+ |                                                       | Animación                     |
|                    |                                                       |  Bucle  (o)   Duración  0.48s |
+--------------------+-------------------------------------------------------+-------------------------------+
| Pistas  [Auto-Key][8 Huesos][+][Duplicar][-] | [|◀][◀](▶)[▶][▶|] 04/29 1.0x | 0f   15f   30f  45f 60f|
|  * BrazoDerecho                              (o) | <>       <>                <>               <>  | 156px
|  * BrazoIzquierdo                            (o) | <>                <>                        <>  |
|  * Espada                                    (o) | <>       <>       <>       <>               <>  |
|  * Cuerpo                                    (o) | <>                                          <>  |
+---------------------------------------------------------------------------------------------------+
  <-     244px     ->  <-                     ~924px (flexible)               ->  <-     272px     ->
```

### Notas estructurales vigentes (`DEC-01`..`DEC-16`)

1. **Superficie continua (`DEC-12`):** cero huecos negros entre paneles y cero `InsetGroup`; las secciones internas del Inspector y Navegador se dividen por aire (`12px`) y divisores horizontales simples de `1px`.
2. **Navegador izquierdo monocromo (`DEC-13`):** fila superior `[ícono monocromo · Arma ▾] < >` para cambiar o curar las 139 armas (con toggle monocromo `Fijar` dentro del desplegable) y grilla vertical de **Clips** debajo sin doble dock inferior.
3. **Riel de 3 herramientas reales (`DEC-11`):** `Mover`, `Rotar`, `Escalar` + toggle de ocultar/mostrar gizmo con iconografía única (`Lucide`/`Phosphor`).
4. **Timeline único (`156px`):** etiquetas completas sin siglas crípticas, 4 pistas visibles sin recorte inferior y marcador dorado de impacto.

---

## 7. Direcciones Artísticas Reales (`DesignSpec` Data-Driven en Rust)

Para cumplir tanto la exigencia real de **estándar Apple Pro (1 identidad coherente, sin caos en toolbar)** como tu pedido de **múltiples direcciones artísticas realmente distintas y no simples recolors**, la arquitectura implementa un **`DesignSpec` plano en Rust** sincronizado con `Theme` y con `ShaderStyleUniform` en WGSL, accesible únicamente desde **Ajustes**:

### Estructura del descriptor `DesignSpec`

- **Geometría:** `radius_panel`, `radius_control`, `radius_button`, `radius_input`, `border_width_panel`, `border_width_control`, `padding_scale`.
- **Materialidad:** `bg_shell`, `bg_panel`, `bg_elevated`, `bg_popover_glass`, `border_structural`, `specular_rim_alpha`, `switch_kind` (`pill_ios` | `toggle_hardware` | `square_cad`), `tab_kind` (`pill` | `underline` | `bracket`).
- **Tipografía:** `font_ui`, `font_mono`, `mono_headers`, `uppercase_micro`.
- **Escenario 3D (`ShaderStyleUniform`):** `stage_bg_top`, `stage_bg_bot`, `stage_grid_kind` (`none` | `soft_perspective` | `engineering_ortho` | `blueprint_double`), `key_light_warmth`, `rim_strength`, `ambient_floor`.

### 5 Direcciones Artísticas Curadas y Radicalmente Distintas (Auditadas por Contraste)

> *Decisión `v0.2` / `v0.3`:* En lugar de 50 skins improvisados o 480 combinaciones sin auditar, el estudio se lanza con **5 Direcciones Hero** donde cambian forma, bordes, tipografía, controles y escenario 3D, más **perfiles combinables avanzados en Ajustes** solo dentro de combinaciones garantizadas WCAG `>= 4.5:1`. *(Si se desea ampliar el catálogo curado a 10–12 presets reales, se agregan como entradas `DesignSpec` auditadas sin inflar Slint.)*

| # | Preset Curado | Geometría | Materialidad y Controles | Tipografía | Escenario 3D (WGSL) |
| :--- | :--- | :--- | :--- | :--- | :--- |
| **1** *(Default)* | **Pro Studio Neutral** *(Estudio Calibrado)* | Sobrio (`9px` paneles, `7px` controles, pills solo en cápsula HUD). | Superficies opacas calibradas `#1c1c1e` / `#242426`, `Tinted Glass` solo en popovers/Spotlight, acento azul `#0a84ff` restringido. | `Plus Jakarta Sans`, sentence case limpio. | Ciclorama neutro `#18191d → #25272e`, grilla sutil exponencial, iluminación 3 puntos neutra + sombra de contacto. |
| **2** | **Field Hardware** *(Instrumento Táctil)* | Seca técnica (`2px` paneles, `2px` botones, cero pills). | Aluminio mate `#18191b` con bordes visibles `#3a3d45`, switches rectangulares de palanca metálica, divisores fuertes. | `JetBrains Mono` en cabeceras, etiquetas en mayúscula técnica. | Mesa oscura mate con retícula ortogonal calibrada, acento naranja señal `#ff5500` y luz dura direccional. |
| **3** | **Daylight Studio** *(Claro Calibrado)* | Orgánica media (`10px` paneles, `7px` controles). | Chrome exterior claro porcelana `#f2f3f7` / `#ffffff`, bordes `#d0d5dd`, texto `#111827`, acento `#0066cc`. | `Plus Jakarta Sans` de alto contraste (`weight 600`). | **Viewport 3D en gris medio calibrado `#2e3138`** (para no invisibilizar estelas VFX ni ejes), con opción manual de ciclorama claro para capturas. |
| **4** | **CAD Blueprint** *(Ingeniería Lineal)* | Ortogonal estricta (`0px` radios, marcos lineales `1px`). | Fondo pizarra cobalto `#0b1526`, paneles wireframe lineales `#274c82`, botones de relleno transparente con contorno nítido. | `JetBrains Mono` dominante en UI y lecturas de ejes. | Escenario azul pizarra con grilla doble `10×10` estilo plano arquitectónico e iluminación fría. |
| **5** | **Studio Clay** *(Editorial Cálido)* | Orgánica suave (`8px` controles, `12px` overlays, `switch_pill`). | Carbón cálido `#151311` / `#1d1a17`, bordes sutiles sin cajas anidadas, acento terracota `#e07a38`. | `Plus Jakarta Sans` limpia. | Ciclorama cálido de estudio de escultura `#181513 → #2a241f`, luz clave cálida + contra fría suave. |

---

## 8. Plan Técnico de Escenario 3D y Corrección Definitiva de `órbita → zoom`

### A. Escenario 3D y Gizmos (`src/shaders.wgsl` + `src/renderer3d.rs`)

1. **Iluminación de 3 puntos realista + Fresnel sutil:** key light con sombra proyectada, fill lateral suave para eliminar caras completamente negras al orbitar y rim calibrado por preset.
2. **Sombra de contacto anclada al piso:** oclusión suave bajo pies y arma para evitar la sensación de flotar sobre el vacío.
3. **Gizmo de transformación limpio:** aro externo discreto en reposo (solo se ilumina al hacer hover), ejes `X/Y/Z` con arcos de rotación nítidos que no tapan el rostro del personaje.

### B. Eliminación de lag `órbita → soltar → zoom` (`src/ui_bridge.rs` + `src/camera.rs`)

1. **Coalescing por frame en `on_vp_pointer_move`:** actualizar `yaw/pitch` en RAM, marcar `dirty.main_viewport = true` + `dirty.triad = true` y encolar como máximo **un** `request_redraw()` por frame (`redraw_queued: Cell<bool>`). Mover `sync_triad_only()` para ejecutarse **1 sola vez por frame** dentro de `BeforeRendering`.
2. **Miniaturas realmente ociosas (1–2 por frame):** reducir `generate_gallery_thumbs_batch(20)` a lotes máximos de `1–2` miniaturas y bloquearlas durante `last_interaction_at.elapsed() < 250ms` y mientras `main_cam.is_actively_zooming()` sea verdadero.
3. **Órbita limpia sin captura accidental de hueso:** orbitar sin disparar edición ni escritura de disco salvo arrastre directo sobre eje de gizmo o clic puntual limpio (`dist < 4px`).
4. **Persistencia fuera del hilo UI:** mover serialización y escritura de `profile.local.json` (`~42 KB`) a worker en segundo plano.
5. **Sin `drop-shadow-blur` dinámico sobre el 3D:** usar bordes de doble tono (`rim especular 1px + marco exterior`) en overlays sobre el visor.
6. **Spike de medición `--bench-orbit-zoom`:** simular `60 eventos de órbita → release → 30 eventos de zoom con rueda` y verificar `p95 < 12ms` (`>= 120 FPS`).

---

### Historial de revisiones del documento

- **`v0.1`:** Diagnóstico inicial de `v1`, referentes comerciales/OSS, propuesta espacial `v2`, 6 direcciones Hero + matriz de 480 combinaciones y plan de rendimiento cámara/GPU.
- **`v0.2`:** Auditoría Red-Team frente a prácticas reales de Apple en 2025–2026 (`macOS 26 Tahoe`, `Final Cut Pro 11`, `Logic Pro`, `Reality Composer Pro`). Se eliminan selector de tema en toolbar y compositor caótico de 480 skins; se enjaula el vidrio translúcido solo a overlays efímeros; se corrige `Clips` de pestaña excluyente a cajón conmutable; se corrige el fondo 3D del modo claro a gris calibrado; se agrega botón `Cámara ▾` + `Vista ▾` y tríada de `68×68px`; se reemplazan abreviaturas por etiquetas claras.
- **`v0.3`:** Inicio de diseño pieza por pieza (Pieza #1 Barra Superior) enfocado en ayudar al flujo de trabajo real del usuario y descartar referentes DCC sobrecargados. Decisiones bloqueadas: `DEC-01` (barra de espacio único sin pestañas ni publicidad de nombre), `DEC-02` (título solo con clip activo y lupa separada `Ctrl+K`) y `DEC-03` (eliminación de botones `Deshacer / Rehacer` de barra superior e inspector).
- **`v0.4`:** Decisión bloqueada `DEC-04` (`Archivo ▾` explícito en esquina superior izquierda con acciones globales de escena, importación, exportación y respaldo de historial).
- **`v0.5`:** Decisión bloqueada `DEC-05` (eliminados los toggles de paneles estilo IDE `[◧] [⬓] [◨]` de la barra superior; expandir el visor `Tab` vive en la esquina del propio viewport 3D).
- **`v0.6`:** Decisión bloqueada `DEC-06` (eliminado el botón visual de lupa `[🔍]` de la barra superior; paleta `Ctrl+K` silenciosa + accesible dentro de `Archivo ▾`).
- **`v0.7`:** Decisión bloqueada `DEC-07` (eliminado botón `Exportar` de la barra superior; exportación completa movida a `Archivo ▾` + `Ctrl+E` y depuradas referencias heredadas en `DEC-01`/`DEC-02`).
- **`v0.8`:** Cierre de `Pieza #1 Barra Superior` (`DEC-08` engranaje `[⚙]` + `DEC-09` estética `Silent Chrome 36px`) y `DEC-10` (Estándar Global de Craft Visual: familia única `Lucide`/`Phosphor`, isotipo geométrico propio, 4 niveles tipográficos y grilla `4px`).
- **`v0.9`:** Inicio de `Pieza #2` con `DEC-11` (riel reducido a `Mover`, `Rotar`, `Escalar` + toggle ocultar/mostrar gizmo con iconografía única).
- **`v0.10`:** Decisión bloqueada `DEC-12` (Arquitectura de Superficie Continua con divisores `1px` eliminando `PanelCard` e `InsetGroup` anidados) e incorporación de la Matriz de Priorización Pareto `80/20` (`80%` enfocado en cámara sin lag, espacio 3D `>= 68%`, iluminación/gizmo y controles sin fricción).
- **`v0.11`:** Decisión bloqueada `DEC-13` (panel izquierdo dedicado a `Clips` con selector monocromo `[Arma ▾] < >` y toggle `Fijar` adentro, eliminando el doble dock inferior y las 139 armas fijas) y actualización íntegra del diagrama en Sección 6.
- **`v0.12`:** Decisión bloqueada `DEC-14` (transporte vectorial mínimo en la cabecera del Timeline, eliminando botones redundantes y despejando el 100% de la parte inferior del Visor 3D).
- **`v0.13`:** Cierre de `Pieza #2 Visor 3D` con `DEC-15` (controles monocromos `[Cámara ▾] [Vista ▾] [⛶]` arriba a la derecha, consolidando `Escena` dentro de `Vista ▾` y omitiendo la tríada XYZ sujeto a prueba real en el prototipo); apertura de `Pieza #3 Inspector Derecho`.
- **`v0.14`:** Cierre de `Pieza #3 Inspector Derecho` con `DEC-16` (2 pestañas `Pose` | `Reglas y VFX` con la lista directa de 8 huesos integrada encima de Transformación + Animación); apertura de `Pieza #4 Timeline`.
- **`v1.0`:** Cierre de `Pieza #4 Timeline` con `DEC-17` y alineación final de los 5 presets curados (`Pro Studio Neutral`, `Field Hardware`, `Daylight Studio`, `CAD Blueprint`, `Studio Clay`). Especificación aprobada lista para ejecución completa `v2`.
- **`v1.1` (`DEC-18`):** Edición granular por hueso en el Timeline sin agregar filas ni altura (`156px`): máscara por hueso `bone_mask: u8` en `Keyframe` con interpolación independiente en `sample_clip_pose`, conmutador discreto `Hueso | Todo` y acciones `Copiar / Pegar / Borrar` en la cabecera del Timeline, selección de rango `Shift+Click` sobre diamantes de una pista, atajos (`Ctrl+C/V` y `Supr` en modo activo; `Ctrl+Shift+C/V` y `Shift+Supr` siempre para pose completa de 8 huesos) y menú contextual por botón derecho en filas de hueso y diamantes (incluye copiar pista completa, pegar espejado y limpiar pista del hueso).
