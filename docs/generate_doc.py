"""
Genera HELIOS_doc.pdf — documentación técnica del proyecto.
Requiere: pip install reportlab
"""
from reportlab.lib.pagesizes import A4
from reportlab.lib.styles import getSampleStyleSheet, ParagraphStyle
from reportlab.lib.units import cm
from reportlab.lib import colors
from reportlab.platypus import (
    SimpleDocTemplate, Paragraph, Spacer, Table, TableStyle, HRFlowable
)
from reportlab.lib.enums import TA_LEFT, TA_CENTER

OUTPUT = "docs/HELIOS_doc.pdf"

doc = SimpleDocTemplate(
    OUTPUT,
    pagesize=A4,
    leftMargin=2.5*cm, rightMargin=2.5*cm,
    topMargin=2.5*cm, bottomMargin=2.5*cm,
)

styles = getSampleStyleSheet()

# ── custom styles ──────────────────────────────────────────────────────────────
title_style = ParagraphStyle(
    "Title2", parent=styles["Title"],
    fontSize=24, spaceAfter=6, textColor=colors.HexColor("#1a1a2e"),
)
subtitle_style = ParagraphStyle(
    "Subtitle", parent=styles["Normal"],
    fontSize=11, spaceAfter=20, textColor=colors.HexColor("#4a4a6a"),
    alignment=TA_CENTER,
)
h1 = ParagraphStyle(
    "H1", parent=styles["Heading1"],
    fontSize=14, spaceBefore=18, spaceAfter=6,
    textColor=colors.HexColor("#1a1a2e"),
    borderPad=4,
)
h2 = ParagraphStyle(
    "H2", parent=styles["Heading2"],
    fontSize=11, spaceBefore=12, spaceAfter=4,
    textColor=colors.HexColor("#2d2d5e"),
)
body = ParagraphStyle(
    "Body2", parent=styles["Normal"],
    fontSize=10, leading=15, spaceAfter=6,
)
code = ParagraphStyle(
    "Code", parent=styles["Code"],
    fontSize=8.5, leading=13, spaceAfter=6,
    backColor=colors.HexColor("#f4f4f8"),
    borderPad=6, leftIndent=12,
)
author_style = ParagraphStyle(
    "Author", parent=styles["Normal"],
    fontSize=10, textColor=colors.HexColor("#666688"),
    alignment=TA_CENTER, spaceAfter=4,
)

# ── document content ───────────────────────────────────────────────────────────
story = []

# Header
story.append(Paragraph("HELIOS-NODE", title_style))
story.append(Paragraph("Controlador predictivo de microrred DC solar", subtitle_style))
story.append(Paragraph("Antonio Zambudio Rodriguez · drizzyrdrgz.exe@protonmail.com", author_style))
story.append(Paragraph("github.com/QuantumDrizzy/HELIOS · Licencia MIT", author_style))
story.append(HRFlowable(width="100%", thickness=1, color=colors.HexColor("#ccccdd"), spaceAfter=16))

# ── Sección 1 ──────────────────────────────────────────────────────────────────
story.append(Paragraph("¿Qué es HELIOS?", h1))
story.append(Paragraph(
    "HELIOS es un controlador predictivo de microrred DC solar construido completamente desde cero. "
    "Combina un controlador MPPT en tiempo real escrito en Rust con un agente de inteligencia artificial "
    "(CNN-LSTM) que predice la irradiancia solar usando datos reales de PVGIS — la base de datos climática "
    "oficial de la Comisión Europea.",
    body))
story.append(Paragraph(
    "El sistema funciona en simulación completa sin necesidad de hardware físico, pero está diseñado "
    "para conectarse a sensores reales (INA219 vía I2C, GPIO en Raspberry Pi o STM32) con mínimas modificaciones.",
    body))

# ── Sección 2 ──────────────────────────────────────────────────────────────────
story.append(Paragraph("Arquitectura", h1))
story.append(Paragraph(
    "Dos procesos independientes se comunican a través de una base de datos SQLite en modo WAL "
    "(Write-Ahead Logging), que actúa como bus IPC de baja latencia:", body))

story.append(Paragraph("Proceso 1 — Rust Core (tick 100 ms)", h2))
story.append(Paragraph(
    "Ejecuta el algoritmo MPPT Perturb &amp; Observe con bias predictivo. Simula la física del panel "
    "(V·I, potencia, duty cycle PWM). Lee el forecast del agente AI desde SQLite cada ciclo y escribe "
    "telemetría completa (V, I, P, duty, SOC, irradiancia) en la base de datos. Mantiene un audit log "
    "SHA-256 encadenado con integridad tamper-evident. Las migraciones de esquema se aplican automáticamente al arrancar.",
    body))

story.append(Paragraph("Proceso 2 — AI Agent Python (tick 1 s)", h2))
story.append(Paragraph(
    "Lee los últimos N registros de telemetría de SQLite. Normaliza la irradiancia real "
    "(irradiance_wm2 / 1100) para inferencia LSTM. Escribe el forecast normalizado [0,1] "
    "en la tabla ai_forecasts. El Rust Core lo lee y ajusta el step del P&amp;O en consecuencia.",
    body))

story.append(Paragraph("Diagrama de flujo", h2))
story.append(Paragraph(
    "PVGIS Data (Murcia, 8760h) → dataset_generator.py → train.py → helios_predictor.pt → "
    "agent.py (--serve) → energy_bus.sqlite ← helios-core (Rust) → dashboard egui",
    code))

# ── Sección 3 ──────────────────────────────────────────────────────────────────
story.append(Paragraph("Modelo LSTM", h1))
story.append(Paragraph(
    "El forecaster combina una capa CNN de extracción de características con capas LSTM para "
    "modelado temporal. El input son secuencias de irradiancia normalizada (GHI/1100) y el output "
    "es el forecast del siguiente periodo, normalizado en [0,1].", body))
story.append(Paragraph(
    "Entrenado con el TMY (Typical Meteorological Year) de PVGIS para Aljucer, Murcia — "
    "8760 horas de irradiancia solar real. El generador de dataset añade perturbaciones de nube "
    "y ruido de sensor para dotar al modelo de robustez ante condiciones reales. "
    "Tiempo de entrenamiento: ~5 minutos en CPU.", body))

# ── Sección 4 ──────────────────────────────────────────────────────────────────
story.append(Paragraph("Datos Reales: PVGIS", h1))
story.append(Paragraph(
    "PVGIS (Photovoltaic Geographical Information System) es la herramienta oficial de la Comisión "
    "Europea para datos solares. HELIOS descarga el TMY para Murcia y lo usa para simular ciclos "
    "día/noche realistas, entrenar el LSTM y alimentar el controlador Rust con irradiancia por hora. "
    "La simulación no es aleatoria — reproduce el comportamiento solar real de Murcia a lo largo del año. "
    "Cambiar la ubicación es un único parámetro en ai/pvgis_client.py.", body))

# ── Sección 5 ──────────────────────────────────────────────────────────────────
story.append(Paragraph("MPPT con Bias Predictivo", h1))
story.append(Paragraph(
    "El P&amp;O clásico es reactivo: perturba el duty cycle y observa si la potencia sube o baja. "
    "HELIOS lo mejora con un bias predictivo:", body))
story.append(Paragraph("duty_step = base_step × (1 + α × forecast)", code))
story.append(Paragraph(
    "Cuando el forecast predice alta irradiancia, el step se amplía para converger antes al MPP. "
    "Cuando predice una caída (nube), el step se reduce para evitar oscilaciones. "
    "El beneficio real aparece en transiciones climáticas, donde el P&amp;O clásico pierde potencia "
    "por perturbaciones en el momento equivocado.", body))

# ── Sección 6 ──────────────────────────────────────────────────────────────────
story.append(Paragraph("Base de Datos SQLite — Esquema", h1))
story.append(Paragraph("5 tablas:", body))

table_data = [
    ["Tabla", "Contenido", "Frecuencia"],
    ["power_telemetry", "V, I, P, duty, SOC, irradiance_wm2", "100 ms"],
    ["ai_forecasts", "forecast [0,1] + inference_time_ms", "1 s"],
    ["material_states", "resultados simulación cuántica (SUBSTRATE)", "bajo demanda"],
    ["system_events", "alertas, cambios de configuración", "eventos"],
    ["audit_log", "log SHA-256 encadenado (tamper-evident)", "cada escritura"],
]
t = Table(table_data, colWidths=[4.5*cm, 8*cm, 3*cm])
t.setStyle(TableStyle([
    ("BACKGROUND", (0,0), (-1,0), colors.HexColor("#1a1a2e")),
    ("TEXTCOLOR",  (0,0), (-1,0), colors.white),
    ("FONTNAME",   (0,0), (-1,0), "Helvetica-Bold"),
    ("FONTSIZE",   (0,0), (-1,-1), 9),
    ("ROWBACKGROUNDS", (0,1), (-1,-1), [colors.HexColor("#f4f4f8"), colors.white]),
    ("GRID", (0,0), (-1,-1), 0.5, colors.HexColor("#ccccdd")),
    ("PADDING", (0,0), (-1,-1), 6),
]))
story.append(t)
story.append(Spacer(1, 12))

# ── Sección 7 ──────────────────────────────────────────────────────────────────
story.append(Paragraph("Stack Tecnológico", h1))

stack_data = [
    ["Capa", "Tecnología"],
    ["Controlador MPPT", "Rust + tokio (async runtime)"],
    ["Dashboard", "egui (immediate mode GUI)"],
    ["Base de datos / IPC", "SQLite WAL + sqlx"],
    ["Audit log", "SHA-256 encadenado (sha2 crate)"],
    ["Post-quantum (investigación)", "ML-KEM 768 + ML-DSA (NIST 2024)"],
    ["AI forecaster", "Python + PyTorch CNN-LSTM"],
    ["Datos solares", "PVGIS API (Comisión Europea)"],
    ["Deploy hardware", "Cross-compile Rust → Raspberry Pi"],
]
t2 = Table(stack_data, colWidths=[6*cm, 9.5*cm])
t2.setStyle(TableStyle([
    ("BACKGROUND", (0,0), (-1,0), colors.HexColor("#1a1a2e")),
    ("TEXTCOLOR",  (0,0), (-1,0), colors.white),
    ("FONTNAME",   (0,0), (-1,0), "Helvetica-Bold"),
    ("FONTSIZE",   (0,0), (-1,-1), 9),
    ("ROWBACKGROUNDS", (0,1), (-1,-1), [colors.HexColor("#f4f4f8"), colors.white]),
    ("GRID", (0,0), (-1,-1), 0.5, colors.HexColor("#ccccdd")),
    ("PADDING", (0,0), (-1,-1), 6),
]))
story.append(t2)
story.append(Spacer(1, 12))

# ── Sección 8 ──────────────────────────────────────────────────────────────────
story.append(Paragraph("Seguridad Post-Cuántica: helios-sentinel", h1))
story.append(Paragraph(
    "helios-sentinel es el daemon de seguridad de HELIOS. Implementa los algoritmos criptográficos "
    "post-cuánticos estandarizados por el NIST en 2024, diseñados para resistir ataques de "
    "computadores cuánticos — incluso ante un adversario con acceso a un ordenador cuántico de gran escala.",
    body))

story.append(Paragraph("Algoritmos implementados", h2))
algo_data = [
    ["Algoritmo", "Estándar NIST", "Función en HELIOS"],
    ["ML-KEM 768", "FIPS 203 (2024)", "Intercambio de claves post-cuántico entre nodos"],
    ["ML-DSA",     "FIPS 204 (2024)", "Firma digital de checkpoints del modelo AI"],
]
t_algo = Table(algo_data, colWidths=[4*cm, 4*cm, 7.5*cm])
t_algo.setStyle(TableStyle([
    ("BACKGROUND", (0,0), (-1,0), colors.HexColor("#1a1a2e")),
    ("TEXTCOLOR",  (0,0), (-1,0), colors.white),
    ("FONTNAME",   (0,0), (-1,0), "Helvetica-Bold"),
    ("FONTSIZE",   (0,0), (-1,-1), 9),
    ("ROWBACKGROUNDS", (0,1), (-1,-1), [colors.HexColor("#f4f4f8"), colors.white]),
    ("GRID", (0,0), (-1,-1), 0.5, colors.HexColor("#ccccdd")),
    ("PADDING", (0,0), (-1,-1), 6),
]))
story.append(t_algo)
story.append(Spacer(1, 10))

story.append(Paragraph("¿Por qué es relevante?", h2))
story.append(Paragraph(
    "Los sistemas de gestión energética almacenan telemetría crítica durante años. "
    "Un atacante con acceso futuro a computación cuántica podría romper la criptografía clásica "
    "(RSA, ECDSA) y falsificar datos históricos retroactivamente. ML-KEM y ML-DSA son resistentes "
    "a este vector de ataque por diseño matemático — están basados en problemas de retículos (lattices) "
    "para los que no existe algoritmo cuántico eficiente conocido.",
    body))

story.append(Paragraph("Función específica en HELIOS", h2))
story.append(Paragraph(
    "helios-sentinel firma los checkpoints del modelo LSTM con ML-DSA antes de guardarlos en disco. "
    "Al arrancar el sistema, verifica la firma antes de cargar el modelo. Si alguien modificó "
    "el archivo del modelo entre sesiones — intencionalmente o por corrupción — el sistema lo detecta "
    "y rechaza el checkpoint. En instalaciones reales donde la telemetría tiene valor legal o regulatorio, "
    "la firma post-cuántica garantiza la cadena de custodia de los datos.",
    body))

story.append(Paragraph("Estado actual", h2))
story.append(Paragraph(
    "helios-sentinel está completamente implementado con criptografía real. "
    "La firma ML-DSA-65 produce 3309 bytes de firma real sobre un hash SHA3-256 del checkpoint. "
    "El seed de 32 bytes se persiste en disco — la clave es estable entre reinicios. "
    "El binario helios-verify permite verificar cualquier checkpoint firmado de forma independiente. "
    "Los tests de roundtrip (sign → verify) pasan en CI. "
    "El único componente que requiere Linux para operar es el Unix Domain Socket server "
    "— diseñado para producción en Raspberry Pi.",
    body))
story.append(Spacer(1, 6))

# ── Sección 9 ──────────────────────────────────────────────────────────────────
story.append(Paragraph("Pasos hacia Hardware Real", h1))
story.append(Paragraph(
    "La arquitectura está diseñada para hardware desde el principio. El paso siguiente "
    "es cablear un sensor INA219 vía I2C y reemplazar las lecturas simuladas en rust/src/main.rs "
    "por lecturas reales del ADC. El repo incluye scripts de cross-compilation y despliegue SSH "
    "para Raspberry Pi listos para usar:", body))
story.append(Paragraph(
    "scripts/cross_compile_rpi.sh  — compila el binario Rust para ARM\n"
    "scripts/deploy_rpi.sh         — despliega vía SSH a la RPi", code))
story.append(Paragraph(
    "Hardware mínimo para instalación real: Raspberry Pi 4, sensor INA219 (I2C), "
    "convertidor DC-DC con control PWM, panel solar + batería.", body))

# ── Sección 9 ──────────────────────────────────────────────────────────────────
story.append(Paragraph("Estado Actual y Limitaciones", h1))
story.append(Paragraph(
    "Lo que funciona: simulación completa día/noche con PVGIS real, forecaster LSTM activo, "
    "dashboard en tiempo real, audit log SHA-256, migraciones de DB automáticas.", body))
story.append(Paragraph(
    "Pendiente: lectura real de ADC/GPIO (INA219), firma ML-DSA real en helios-sentinel "
    "(actualmente mock), tests de integración y CI.", body))

# ── Footer ─────────────────────────────────────────────────────────────────────
story.append(Spacer(1, 20))
story.append(HRFlowable(width="100%", thickness=1, color=colors.HexColor("#ccccdd"), spaceAfter=8))
story.append(Paragraph(
    "github.com/QuantumDrizzy/HELIOS · MIT License · Antonio Zambudio Rodriguez · 2026",
    author_style))

# ── Build ──────────────────────────────────────────────────────────────────────
doc.build(story)
print(f"PDF generado: {OUTPUT}")
