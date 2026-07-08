import React, { useState } from 'react'
import * as d3 from 'd3'

function App() {
  const [weights, setWeights] = useState({ vram: 35, load: 25, slots: 15, geo: 15 })

  const nodes = [
    { id: "node-paris", vram: 24, load: 0.25, slots: 5 },
    { id: "node-frankfurt", vram: 20, load: 0.35, slots: 6 },
    { id: "node-nyc", vram: 32, load: 0.55, slots: 4 },
    { id: "node-tokyo", vram: 18, load: 0.15, slots: 7 },
    { id: "node-singapore", vram: 22, load: 0.40, slots: 5 },
  ]

  const calculateBreakdown = (node) => {
    const vramS = Math.min(node.vram / 24, 1) * weights.vram
    const loadS = (1 - Math.min(Math.max(node.load, 0), 1)) * weights.load
    const slotsS = Math.min(node.slots / 8, 1) * weights.slots
    const geoS = weights.geo * 0.6
    return {
      id: node.id,
      vram: Math.round(vramS),
      load: Math.round(loadS),
      slots: Math.round(slotsS),
      geo: Math.round(geoS),
      total: Math.round(vramS + loadS + slotsS + geoS),
    }
  }

  const data = nodes.map(calculateBreakdown).sort((a, b) => b.total - a.total)

  return (
    <div style={{ padding: '40px', maxWidth: '1200px', margin: '0 auto', color: '#f0eff8', background: '#0a0a0f', minHeight: '100vh' }}>
      <h1 style={{ color: '#7c6deb' }}>AInonymous Node Scoring Dashboard</h1>
      <p>React + D3.js Interactive Visualizer</p>

      <div style={{ marginBottom: '30px' }}>
        {Object.keys(weights).map(key => (
          <div key={key} style={{ marginBottom: '12px' }}>
            <label style={{ display: 'inline-block', width: '80px' }}>{key.toUpperCase()}</label>
            <input
              type="range"
              min="0"
              max="50"
              value={weights[key]}
              onChange={(e) => setWeights({ ...weights, [key]: +e.target.value })}
            />
            <span style={{ marginLeft: '12px', color: '#5dd8a8', fontWeight: '600' }}>{weights[key]}%</span>
          </div>
        ))}
      </div>

      <div style={{ background: '#12121a', padding: '30px', borderRadius: '12px' }}>
        <h3 style={{ marginBottom: '20px' }}>Classement des nœuds</h3>
        {data.map((d, i) => (
          <div key={i} style={{ 
            background: '#1a1a28', 
            padding: '16px', 
            borderRadius: '8px', 
            marginBottom: '12px',
            display: 'flex',
            justifyContent: 'space-between',
            alignItems: 'center'
          }}>
            <div>
              <strong>#{i + 1} {d.id}</strong>
            </div>
            <div style={{ fontSize: '22px', fontWeight: '700', color: '#5dd8a8' }}>
              {d.total}
            </div>
          </div>
        ))}
      </div>

      <p style={{ marginTop: '40px', color: '#8887a0', fontSize: '14px' }}>
        Modifie les poids ci-dessus pour voir l'impact en temps réel sur le classement.
      </p>
    </div>
  )
}

export default App
