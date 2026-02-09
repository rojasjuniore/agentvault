use anchor_lang::prelude::*;

declare_id!("AgntVLT1111111111111111111111111111111111111");

#[program]
pub mod agentvault {
    use super::*;

    /// Register a new agent in the registry
    pub fn register_agent(
        ctx: Context<RegisterAgent>,
        name: String,
        metadata_uri: String,
        skills: Vec<String>,
    ) -> Result<()> {
        require!(name.len() <= 32, AgentVaultError::NameTooLong);
        require!(metadata_uri.len() <= 200, AgentVaultError::MetadataUriTooLong);
        require!(skills.len() <= 10, AgentVaultError::TooManySkills);

        let agent = &mut ctx.accounts.agent_profile;
        let clock = Clock::get()?;

        agent.wallet = ctx.accounts.owner.key();
        agent.name = name;
        agent.metadata_uri = metadata_uri;
        agent.skills = skills;
        agent.reputation = 50; // Base reputation
        agent.endorsements_received = 0;
        agent.registered_at = clock.unix_timestamp;
        agent.last_active = clock.unix_timestamp;
        agent.bump = ctx.bumps.agent_profile;

        // Update registry stats
        let stats = &mut ctx.accounts.registry_stats;
        stats.total_agents += 1;

        emit!(AgentRegistered {
            wallet: agent.wallet,
            name: agent.name.clone(),
            timestamp: clock.unix_timestamp,
        });

        Ok(())
    }

    /// Update an existing agent profile
    pub fn update_profile(
        ctx: Context<UpdateProfile>,
        metadata_uri: Option<String>,
        skills: Option<Vec<String>>,
    ) -> Result<()> {
        let agent = &mut ctx.accounts.agent_profile;
        let clock = Clock::get()?;

        if let Some(uri) = metadata_uri {
            require!(uri.len() <= 200, AgentVaultError::MetadataUriTooLong);
            agent.metadata_uri = uri;
        }

        if let Some(new_skills) = skills {
            require!(new_skills.len() <= 10, AgentVaultError::TooManySkills);
            agent.skills = new_skills;
        }

        agent.last_active = clock.unix_timestamp;

        emit!(ProfileUpdated {
            wallet: agent.wallet,
            timestamp: clock.unix_timestamp,
        });

        Ok(())
    }

    /// Endorse another agent's skill
    pub fn endorse_skill(
        ctx: Context<EndorseSkill>,
        skill: String,
    ) -> Result<()> {
        require!(skill.len() <= 32, AgentVaultError::SkillNameTooLong);
        
        // Can't endorse yourself
        require!(
            ctx.accounts.endorser.key() != ctx.accounts.target_agent.wallet,
            AgentVaultError::CannotEndorseSelf
        );

        // Target must have this skill declared
        require!(
            ctx.accounts.target_agent.skills.contains(&skill),
            AgentVaultError::SkillNotDeclared
        );

        let endorsement = &mut ctx.accounts.endorsement;
        let clock = Clock::get()?;

        endorsement.endorser = ctx.accounts.endorser.key();
        endorsement.target = ctx.accounts.target_agent.wallet;
        endorsement.skill = skill.clone();
        endorsement.timestamp = clock.unix_timestamp;
        endorsement.bump = ctx.bumps.endorsement;

        // Update target's endorsement count and reputation
        let target = &mut ctx.accounts.target_agent;
        target.endorsements_received += 1;
        
        // Reputation boost: min(100, current + 2)
        target.reputation = std::cmp::min(100, target.reputation + 2);
        target.last_active = clock.unix_timestamp;

        // Update endorser's last active
        let endorser_profile = &mut ctx.accounts.endorser_profile;
        endorser_profile.last_active = clock.unix_timestamp;

        emit!(SkillEndorsed {
            endorser: endorsement.endorser,
            target: endorsement.target,
            skill,
            timestamp: clock.unix_timestamp,
        });

        Ok(())
    }

    /// Revoke a previously given endorsement
    pub fn revoke_endorsement(ctx: Context<RevokeEndorsement>) -> Result<()> {
        let endorsement = &ctx.accounts.endorsement;
        let target = &mut ctx.accounts.target_agent;
        let clock = Clock::get()?;

        // Decrease endorsement count
        target.endorsements_received = target.endorsements_received.saturating_sub(1);
        
        // Reputation penalty: max(0, current - 2)
        target.reputation = target.reputation.saturating_sub(2);

        emit!(EndorsementRevoked {
            endorser: endorsement.endorser,
            target: endorsement.target,
            skill: endorsement.skill.clone(),
            timestamp: clock.unix_timestamp,
        });

        Ok(())
    }

    /// Initialize the registry (one-time setup)
    pub fn initialize_registry(ctx: Context<InitializeRegistry>) -> Result<()> {
        let stats = &mut ctx.accounts.registry_stats;
        stats.total_agents = 0;
        stats.total_endorsements = 0;
        stats.authority = ctx.accounts.authority.key();
        stats.bump = ctx.bumps.registry_stats;
        Ok(())
    }
}

// ============================================================================
// Accounts
// ============================================================================

#[derive(Accounts)]
pub struct InitializeRegistry<'info> {
    #[account(
        init,
        payer = authority,
        space = 8 + RegistryStats::INIT_SPACE,
        seeds = [b"registry_stats"],
        bump
    )]
    pub registry_stats: Account<'info, RegistryStats>,
    
    #[account(mut)]
    pub authority: Signer<'info>,
    
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
#[instruction(name: String)]
pub struct RegisterAgent<'info> {
    #[account(
        init,
        payer = owner,
        space = 8 + AgentProfile::INIT_SPACE,
        seeds = [b"agent", owner.key().as_ref()],
        bump
    )]
    pub agent_profile: Account<'info, AgentProfile>,
    
    #[account(
        mut,
        seeds = [b"registry_stats"],
        bump = registry_stats.bump
    )]
    pub registry_stats: Account<'info, RegistryStats>,
    
    #[account(mut)]
    pub owner: Signer<'info>,
    
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct UpdateProfile<'info> {
    #[account(
        mut,
        seeds = [b"agent", owner.key().as_ref()],
        bump = agent_profile.bump,
        has_one = wallet @ AgentVaultError::Unauthorized
    )]
    pub agent_profile: Account<'info, AgentProfile>,
    
    #[account(mut, constraint = owner.key() == agent_profile.wallet)]
    pub owner: Signer<'info>,
}

#[derive(Accounts)]
#[instruction(skill: String)]
pub struct EndorseSkill<'info> {
    #[account(
        init,
        payer = endorser,
        space = 8 + Endorsement::INIT_SPACE,
        seeds = [
            b"endorsement",
            endorser.key().as_ref(),
            target_agent.wallet.as_ref(),
            skill.as_bytes()
        ],
        bump
    )]
    pub endorsement: Account<'info, Endorsement>,
    
    #[account(
        mut,
        seeds = [b"agent", endorser.key().as_ref()],
        bump = endorser_profile.bump
    )]
    pub endorser_profile: Account<'info, AgentProfile>,
    
    #[account(
        mut,
        seeds = [b"agent", target_agent.wallet.as_ref()],
        bump = target_agent.bump
    )]
    pub target_agent: Account<'info, AgentProfile>,
    
    #[account(mut)]
    pub endorser: Signer<'info>,
    
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct RevokeEndorsement<'info> {
    #[account(
        mut,
        close = endorser,
        seeds = [
            b"endorsement",
            endorser.key().as_ref(),
            target_agent.wallet.as_ref(),
            endorsement.skill.as_bytes()
        ],
        bump = endorsement.bump,
        has_one = endorser
    )]
    pub endorsement: Account<'info, Endorsement>,
    
    #[account(
        mut,
        seeds = [b"agent", target_agent.wallet.as_ref()],
        bump = target_agent.bump
    )]
    pub target_agent: Account<'info, AgentProfile>,
    
    #[account(mut)]
    pub endorser: Signer<'info>,
}

// ============================================================================
// State
// ============================================================================

#[account]
#[derive(InitSpace)]
pub struct RegistryStats {
    pub total_agents: u64,
    pub total_endorsements: u64,
    pub authority: Pubkey,
    pub bump: u8,
}

#[account]
#[derive(InitSpace)]
pub struct AgentProfile {
    pub wallet: Pubkey,
    #[max_len(32)]
    pub name: String,
    #[max_len(200)]
    pub metadata_uri: String,
    #[max_len(10, 32)]
    pub skills: Vec<String>,
    pub reputation: u8,           // 0-100
    pub endorsements_received: u32,
    pub registered_at: i64,
    pub last_active: i64,
    pub bump: u8,
}

#[account]
#[derive(InitSpace)]
pub struct Endorsement {
    pub endorser: Pubkey,
    pub target: Pubkey,
    #[max_len(32)]
    pub skill: String,
    pub timestamp: i64,
    pub bump: u8,
}

// ============================================================================
// Events
// ============================================================================

#[event]
pub struct AgentRegistered {
    pub wallet: Pubkey,
    pub name: String,
    pub timestamp: i64,
}

#[event]
pub struct ProfileUpdated {
    pub wallet: Pubkey,
    pub timestamp: i64,
}

#[event]
pub struct SkillEndorsed {
    pub endorser: Pubkey,
    pub target: Pubkey,
    pub skill: String,
    pub timestamp: i64,
}

#[event]
pub struct EndorsementRevoked {
    pub endorser: Pubkey,
    pub target: Pubkey,
    pub skill: String,
    pub timestamp: i64,
}

// ============================================================================
// Errors
// ============================================================================

#[error_code]
pub enum AgentVaultError {
    #[msg("Name must be 32 characters or less")]
    NameTooLong,
    #[msg("Metadata URI must be 200 characters or less")]
    MetadataUriTooLong,
    #[msg("Cannot declare more than 10 skills")]
    TooManySkills,
    #[msg("Skill name must be 32 characters or less")]
    SkillNameTooLong,
    #[msg("Cannot endorse yourself")]
    CannotEndorseSelf,
    #[msg("Target agent has not declared this skill")]
    SkillNotDeclared,
    #[msg("Unauthorized")]
    Unauthorized,
}
