use super::{
    background::BackgroundPool,
    model::*,
    repository::TunnelRuleRepository,
    runtime::{self, LiveTunnel},
};
use crate::domain::{AppError, AppResult};
use std::{collections::HashMap, path::PathBuf};
use tokio::sync::{watch, Mutex, Semaphore};
use uuid::Uuid;

pub(super) struct TunnelData {
    pub(super) rules: Vec<TunnelRule>,
    pub(super) live: HashMap<Uuid, LiveTunnel>,
    pub(super) failures: HashMap<Uuid, TunnelStatus>,
    pub(super) pending: HashMap<Uuid, (Uuid, watch::Sender<bool>)>,
}

pub struct TunnelService {
    repository: TunnelRuleRepository,
    pub(super) data: Mutex<Option<TunnelData>>,
    pub(super) background: BackgroundPool,
    probes: Semaphore,
}

impl TunnelService {
    pub fn at_path(path: PathBuf) -> Self {
        Self {
            repository: TunnelRuleRepository::new(path),
            data: Mutex::new(None),
            background: BackgroundPool::default(),
            probes: Semaphore::new(4),
        }
    }
    pub(super) async fn load(&self, data: &mut Option<TunnelData>) -> AppResult<()> {
        if data.is_none() {
            *data = Some(TunnelData {
                rules: self.repository.load().await?,
                live: HashMap::new(),
                failures: HashMap::new(),
                pending: HashMap::new(),
            });
        }
        Ok(())
    }
    pub async fn list(&self) -> AppResult<Vec<TunnelView>> {
        let mut guard = self.data.lock().await;
        self.load(&mut guard).await?;
        let data = guard.as_ref().ok_or(AppError::Storage)?;
        let mut views = Vec::new();
        for rule in &data.rules {
            let status = match data.live.get(&rule.id) {
                Some(live) => live.snapshot().await,
                None => data.failures.get(&rule.id).cloned().unwrap_or_default(),
            };
            views.push(TunnelView {
                rule: rule.clone(),
                status,
            });
        }
        Ok(views)
    }
    pub async fn rule(&self, id: Uuid) -> AppResult<TunnelRule> {
        self.list()
            .await?
            .into_iter()
            .find(|view| view.rule.id == id)
            .map(|view| view.rule)
            .ok_or(AppError::TunnelNotFound)
    }
    pub async fn save(&self, request: SaveTunnelRequest) -> AppResult<TunnelRule> {
        let rule = TunnelRule {
            id: request.id.unwrap_or_else(Uuid::new_v4),
            name: request.name.trim().into(),
            profile_id: request.profile_id,
            target_host: request.target_host.trim().into(),
            target_port: request.target_port,
            local_port: request.local_port,
        };
        rule.validate()?;
        let mut guard = self.data.lock().await;
        self.load(&mut guard).await?;
        let data = guard.as_mut().ok_or(AppError::Storage)?;
        ensure_stopped(data, rule.id).await?;
        let mut updated = data.rules.clone();
        if request.id.is_some() {
            let existing = updated
                .iter_mut()
                .find(|item| item.id == rule.id)
                .ok_or(AppError::TunnelNotFound)?;
            *existing = rule.clone();
        } else {
            if updated.len() >= 100 {
                return Err(AppError::TunnelLimit);
            }
            updated.push(rule.clone());
        }
        self.repository.save(&updated).await?;
        data.rules = updated;
        if let Some(mut live) = data.live.remove(&rule.id) {
            live.shutdown().await;
        }
        data.failures.remove(&rule.id);
        Ok(rule)
    }
    pub async fn delete(&self, id: Uuid) -> AppResult<()> {
        let mut guard = self.data.lock().await;
        self.load(&mut guard).await?;
        let data = guard.as_mut().ok_or(AppError::Storage)?;
        ensure_stopped(data, id).await?;
        if !data.rules.iter().any(|rule| rule.id == id) {
            return Err(AppError::TunnelNotFound);
        }
        let updated: Vec<_> = data
            .rules
            .iter()
            .filter(|rule| rule.id != id)
            .cloned()
            .collect();
        self.repository.save(&updated).await?;
        data.rules = updated;
        if let Some(mut live) = data.live.remove(&id) {
            live.shutdown().await;
        }
        data.failures.remove(&id);
        Ok(())
    }
    pub async fn stop(&self, id: Uuid) -> AppResult<()> {
        let mut guard = self.data.lock().await;
        self.load(&mut guard).await?;
        let data = guard.as_mut().ok_or(AppError::Storage)?;
        if !data.rules.iter().any(|rule| rule.id == id) {
            return Err(AppError::TunnelNotFound);
        }
        data.pending.remove(&id);
        if let Some(live) = data.live.get_mut(&id) {
            live.shutdown().await;
        }
        data.failures.remove(&id);
        Ok(())
    }
    pub async fn stop_session(&self, session_id: Uuid) {
        let mut guard = self.data.lock().await;
        if let Some(data) = guard.as_mut() {
            for live in data.live.values_mut() {
                if live.snapshot().await.session_id == Some(session_id) {
                    live.shutdown().await;
                }
            }
        }
    }
    pub async fn session_impact(&self, session_id: Uuid) -> Vec<TunnelRule> {
        // Runtime-only: damaged saved rules must not prevent an SSH disconnect.
        let guard = self.data.lock().await;
        let mut affected = Vec::new();
        if let Some(data) = guard.as_ref() {
            for rule in &data.rules {
                if let Some(live) = data.live.get(&rule.id) {
                    let status = live.snapshot().await;
                    if status.session_id == Some(session_id) && status.state == TunnelState::Running
                    {
                        affected.push(rule.clone());
                    }
                }
            }
        }
        affected
    }
    pub async fn check(&self, id: Uuid, authorized_profile: Uuid) -> AppResult<()> {
        let _permit = self
            .probes
            .try_acquire()
            .map_err(|_| AppError::TunnelLimit)?;
        let (rule, transport, status, stop) = {
            let mut guard = self.data.lock().await;
            self.load(&mut guard).await?;
            let data = guard.as_ref().ok_or(AppError::Storage)?;
            let rule = data
                .rules
                .iter()
                .find(|rule| rule.id == id)
                .cloned()
                .ok_or(AppError::TunnelNotFound)?;
            if rule.profile_id != authorized_profile {
                return Err(AppError::TunnelSessionMismatch);
            }
            let live = data.live.get(&id).ok_or(AppError::TunnelStopped)?;
            if live.snapshot().await.state != TunnelState::Running {
                return Err(AppError::TunnelStopped);
            }
            (
                rule,
                live.transport.clone(),
                live.status.clone(),
                live.stop.subscribe(),
            )
        };
        runtime::probe(&rule, transport, status, stop).await
    }
}

pub(super) async fn ensure_stopped(data: &TunnelData, id: Uuid) -> AppResult<()> {
    if data.pending.contains_key(&id) {
        return Err(AppError::TunnelRunning);
    }
    if let Some(live) = data.live.get(&id) {
        if live.snapshot().await.state == TunnelState::Running {
            return Err(AppError::TunnelRunning);
        }
    }
    Ok(())
}
