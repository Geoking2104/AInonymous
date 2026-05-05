import express from 'express'
import cors from 'cors'
import 'dotenv/config'

const app = express()
app.use(cors())
app.use(express.json())

const PORT        = process.env.PORT || 3001
const MODEL       = process.env.MODEL || 'qwen3:32b'
const OLLAMA_URL  = process.env.OLLAMA_BASE_URL || 'http://localhost:11434'
const API_BASE    = process.env.API_BASE_URL   // défini => mode API externe
const API_KEY     = process.env.API_KEY || ''

// ─── System prompt orienté génération de code ────────────────────────────────
const SYSTEM_PROMPT = `You are an expert software engineer and code generation assistant powered by Qwen3.
Your role is to write clean, production-quality code based on user requests.

Rules:
- Always wrap code blocks in triple backticks with the language tag (e.g. \`\`\`typescript)
- Provide concise explanations before and after code blocks when helpful
- Prefer modern, idiomatic patterns for the target language
- If multiple files are needed, clearly label each one with a comment header
- Point out potential edge cases or improvements when relevant
- Think step by step before writing code when the problem is complex
- Respond in the same language the user uses (French → French, English → English)`

// ─── Helper : stream Ollama (/api/chat) ──────────────────────────────────────
async function streamOllama(messages, res) {
  const body = JSON.stringify({
    model: MODEL,
    messages,
    stream: true,
    options: {
      temperature: 0.6,
      top_p: 0.95,
      num_predict: 4096,
    },
    // Qwen3 supporte le "thinking" mode — désactivé ici pour la vitesse
    think: false,
  })

  const upstream = await fetch(`${OLLAMA_URL}/api/chat`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body,
  })

  if (!upstream.ok) {
    const err = await upstream.text()
    res.write(`data: ${JSON.stringify({ error: err })}\n\n`)
    res.end()
    return
  }

  const reader = upstream.body.getReader()
  const decoder = new TextDecoder()

  while (true) {
    const { value, done } = await reader.read()
    if (done) break

    const chunk = decoder.decode(value, { stream: true })
    // Ollama renvoie des lignes JSON séparées par \n
    for (const line of chunk.split('\n').filter(Boolean)) {
      try {
        const json = JSON.parse(line)
        const token = json?.message?.content ?? ''
        if (token) res.write(`data: ${JSON.stringify({ token })}\n\n`)
        if (json.done) {
          res.write(`data: ${JSON.stringify({ done: true })}\n\n`)
          res.end()
          return
        }
      } catch {
        // ligne incomplète — ignorée
      }
    }
  }
  res.write(`data: ${JSON.stringify({ done: true })}\n\n`)
  res.end()
}

// ─── Helper : stream API OpenAI-compatible ───────────────────────────────────
async function streamOpenAI(messages, res) {
  const body = JSON.stringify({
    model: MODEL,
    messages,
    stream: true,
    temperature: 0.6,
    top_p: 0.95,
    max_tokens: 4096,
  })

  const upstream = await fetch(`${API_BASE}/chat/completions`, {
    method: 'POST',
    headers: {
      'Content-Type': 'application/json',
      Authorization: `Bearer ${API_KEY}`,
    },
    body,
  })

  if (!upstream.ok) {
    const err = await upstream.text()
    res.write(`data: ${JSON.stringify({ error: err })}\n\n`)
    res.end()
    return
  }

  const reader = upstream.body.getReader()
  const decoder = new TextDecoder()

  while (true) {
    const { value, done } = await reader.read()
    if (done) break

    const chunk = decoder.decode(value, { stream: true })
    for (const line of chunk.split('\n').filter(l => l.startsWith('data: '))) {
      const data = line.slice(6).trim()
      if (data === '[DONE]') {
        res.write(`data: ${JSON.stringify({ done: true })}\n\n`)
        res.end()
        return
      }
      try {
        const json = JSON.parse(data)
        const token = json.choices?.[0]?.delta?.content ?? ''
        if (token) res.write(`data: ${JSON.stringify({ token })}\n\n`)
      } catch {
        // ignoré
      }
    }
  }
  res.write(`data: ${JSON.stringify({ done: true })}\n\n`)
  res.end()
}

// ─── POST /api/generate ──────────────────────────────────────────────────────
app.post('/api/generate', async (req, res) => {
  const { messages } = req.body

  if (!messages || !Array.isArray(messages)) {
    return res.status(400).json({ error: 'messages[] requis' })
  }

  // Injecter le system prompt en tête si absent
  const fullMessages = messages[0]?.role === 'system'
    ? messages
    : [{ role: 'system', content: SYSTEM_PROMPT }, ...messages]

  // SSE headers
  res.setHeader('Content-Type', 'text/event-stream')
  res.setHeader('Cache-Control', 'no-cache')
  res.setHeader('Connection', 'keep-alive')
  res.flushHeaders()

  try {
    if (API_BASE) {
      await streamOpenAI(fullMessages, res)
    } else {
      await streamOllama(fullMessages, res)
    }
  } catch (err) {
    console.error('Streaming error:', err)
    res.write(`data: ${JSON.stringify({ error: err.message })}\n\n`)
    res.end()
  }
})

// ─── GET /api/health ─────────────────────────────────────────────────────────
app.get('/api/health', async (_req, res) => {
  try {
    if (API_BASE) {
      return res.json({ status: 'ok', mode: 'api', model: MODEL })
    }
    const r = await fetch(`${OLLAMA_URL}/api/tags`)
    const data = await r.json()
    const models = data.models?.map(m => m.name) ?? []
    const available = models.includes(MODEL)
    res.json({ status: 'ok', mode: 'ollama', model: MODEL, available, models })
  } catch (err) {
    res.status(503).json({ status: 'error', message: err.message })
  }
})

app.listen(PORT, () => {
  console.log(`\n🚀 Qwen3 backend running on http://localhost:${PORT}`)
  console.log(`   Mode  : ${API_BASE ? 'API externe (' + API_BASE + ')' : 'Ollama (' + OLLAMA_URL + ')'}`)
  console.log(`   Modèle: ${MODEL}\n`)
})
