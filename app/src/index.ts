import express from 'express';
import cors from 'cors';
import { Connection, PublicKey, Keypair } from '@solana/web3.js';
import * as anchor from '@coral-xyz/anchor';

const app = express();
app.use(cors());
app.use(express.json());

const PORT = process.env.PORT || 3000;
const RPC_URL = process.env.SOLANA_RPC_URL || 'https://api.devnet.solana.com';
const PROGRAM_ID = new PublicKey('AgntVLT1111111111111111111111111111111111111');

const connection = new Connection(RPC_URL, 'confirmed');

// In-memory cache for demo (replace with on-chain reads in production)
interface Agent {
  wallet: string;
  name: string;
  skills: string[];
  reputation: number;
  endorsements: number;
  registeredAt: string;
  lastActive: string;
  metadataUri?: string;
}

interface Endorsement {
  id: string;
  endorser: string;
  target: string;
  skill: string;
  timestamp: string;
}

const agents: Map<string, Agent> = new Map();
const endorsements: Map<string, Endorsement> = new Map();

// ============================================================================
// Routes
// ============================================================================

// Health check
app.get('/health', (req, res) => {
  res.json({ status: 'ok', version: '0.1.0' });
});

// SKILL.md endpoint
app.get('/skill.md', (req, res) => {
  res.sendFile('SKILL.md', { root: '..' });
});

// Get registry stats
app.get('/api/stats', (req, res) => {
  res.json({
    totalAgents: agents.size,
    totalEndorsements: endorsements.size,
    programId: PROGRAM_ID.toBase58(),
    network: 'devnet'
  });
});

// Register agent
app.post('/api/agents', async (req, res) => {
  try {
    const { name, skills, metadataUri } = req.body;
    const authHeader = req.headers.authorization;
    
    if (!authHeader || !authHeader.startsWith('Bearer ')) {
      return res.status(401).json({ error: 'Missing AgentWallet token' });
    }

    // In production: verify token with AgentWallet API
    // For demo: generate a temporary wallet
    const wallet = Keypair.generate().publicKey.toBase58();

    if (!name || typeof name !== 'string') {
      return res.status(400).json({ error: 'Name is required' });
    }

    if (!skills || !Array.isArray(skills) || skills.length === 0) {
      return res.status(400).json({ error: 'At least one skill is required' });
    }

    if (skills.length > 10) {
      return res.status(400).json({ error: 'Maximum 10 skills allowed' });
    }

    const now = new Date().toISOString();
    const agent: Agent = {
      wallet,
      name,
      skills,
      reputation: 50,
      endorsements: 0,
      registeredAt: now,
      lastActive: now,
      metadataUri
    };

    agents.set(wallet, agent);

    res.status(201).json({
      success: true,
      agent,
      signature: 'simulated_' + Date.now() // In production: actual tx signature
    });
  } catch (error) {
    console.error('Register error:', error);
    res.status(500).json({ error: 'Internal server error' });
  }
});

// List/search agents
app.get('/api/agents', (req, res) => {
  const { skill, minReputation, limit = '50' } = req.query;

  let results = Array.from(agents.values());

  if (skill && typeof skill === 'string') {
    results = results.filter(a => a.skills.includes(skill));
  }

  if (minReputation) {
    const minRep = parseInt(minReputation as string, 10);
    results = results.filter(a => a.reputation >= minRep);
  }

  // Sort by reputation descending
  results.sort((a, b) => b.reputation - a.reputation);

  // Limit
  const maxResults = Math.min(parseInt(limit as string, 10), 100);
  results = results.slice(0, maxResults);

  res.json({
    agents: results,
    total: results.length
  });
});

// Get specific agent
app.get('/api/agents/:wallet', (req, res) => {
  const { wallet } = req.params;
  const agent = agents.get(wallet);

  if (!agent) {
    return res.status(404).json({ error: 'Agent not found' });
  }

  // Get endorsements for this agent
  const agentEndorsements = Array.from(endorsements.values())
    .filter(e => e.target === wallet);

  res.json({
    ...agent,
    endorsementDetails: agentEndorsements
  });
});

// Update agent profile
app.patch('/api/agents/:wallet', (req, res) => {
  const { wallet } = req.params;
  const { skills, metadataUri } = req.body;

  const agent = agents.get(wallet);
  if (!agent) {
    return res.status(404).json({ error: 'Agent not found' });
  }

  // In production: verify ownership via signature
  if (skills && Array.isArray(skills)) {
    if (skills.length > 10) {
      return res.status(400).json({ error: 'Maximum 10 skills allowed' });
    }
    agent.skills = skills;
  }

  if (metadataUri) {
    agent.metadataUri = metadataUri;
  }

  agent.lastActive = new Date().toISOString();
  agents.set(wallet, agent);

  res.json({ success: true, agent });
});

// Create endorsement
app.post('/api/endorsements', (req, res) => {
  const { targetWallet, skill } = req.body;
  const authHeader = req.headers.authorization;

  if (!authHeader || !authHeader.startsWith('Bearer ')) {
    return res.status(401).json({ error: 'Missing AgentWallet token' });
  }

  // In production: get endorser wallet from token
  const endorserWallet = Keypair.generate().publicKey.toBase58();

  const target = agents.get(targetWallet);
  if (!target) {
    return res.status(404).json({ error: 'Target agent not found' });
  }

  if (!target.skills.includes(skill)) {
    return res.status(400).json({ error: 'Target has not declared this skill' });
  }

  const id = `${endorserWallet}_${targetWallet}_${skill}`;
  if (endorsements.has(id)) {
    return res.status(409).json({ error: 'Already endorsed this skill' });
  }

  const endorsement: Endorsement = {
    id,
    endorser: endorserWallet,
    target: targetWallet,
    skill,
    timestamp: new Date().toISOString()
  };

  endorsements.set(id, endorsement);

  // Update target reputation
  target.endorsements += 1;
  target.reputation = Math.min(100, target.reputation + 2);
  target.lastActive = new Date().toISOString();
  agents.set(targetWallet, target);

  res.status(201).json({
    success: true,
    endorsement,
    signature: 'simulated_' + Date.now()
  });
});

// Revoke endorsement
app.delete('/api/endorsements/:id', (req, res) => {
  const { id } = req.params;
  
  const endorsement = endorsements.get(id);
  if (!endorsement) {
    return res.status(404).json({ error: 'Endorsement not found' });
  }

  // In production: verify endorser ownership
  endorsements.delete(id);

  // Update target reputation
  const target = agents.get(endorsement.target);
  if (target) {
    target.endorsements = Math.max(0, target.endorsements - 1);
    target.reputation = Math.max(0, target.reputation - 2);
    agents.set(endorsement.target, target);
  }

  res.json({ success: true, message: 'Endorsement revoked' });
});

// ============================================================================
// Start server
// ============================================================================

app.listen(PORT, () => {
  console.log(`AgentVault API running on port ${PORT}`);
  console.log(`Network: devnet`);
  console.log(`Program ID: ${PROGRAM_ID.toBase58()}`);
});
