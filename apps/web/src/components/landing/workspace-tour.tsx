"use client";

import { useRef, useState, type KeyboardEvent } from "react";
import { Activity, ArrowDownToLine, Box, Check, ChevronDown, CircleHelp, FileCode2, Folder, GitBranch, HardDrive, LayoutGrid, LockKeyhole, Search, Server, Settings2, Terminal, X } from "lucide-react";
import { BrandMark } from "@/components/brand-mark";
import type { LandingCopy } from "@/lib/landing-copy";

const icons = [Terminal, Folder, Activity, GitBranch];
const files = [["app", "—"], ["public", "—"], ["logs", "—"], ["docker-compose.yml", "1.2 KB"], ["README.md", "3.4 KB"]];

export function WorkspaceTour({ copy: t }: { copy: LandingCopy["demo"] }) {
  const [active, setActive] = useState(0);
  const tabs = useRef<(HTMLButtonElement | null)[]>([]);
  function navigate(event: KeyboardEvent<HTMLButtonElement>, index: number) {
    let next = index;
    if (event.key === "ArrowRight") next = (index + 1) % icons.length;
    else if (event.key === "ArrowLeft") next = (index + icons.length - 1) % icons.length;
    else if (event.key === "Home") next = 0;
    else if (event.key === "End") next = icons.length - 1;
    else return;
    event.preventDefault(); setActive(next); tabs.current[next]?.focus();
  }
  return <div className="tour-wrap" id="workspace">
    <div className="tour-heading"><span><span className="live-dot" />{t.label}</span><span>{t.caption}</span></div>
    <div className="workspace-tour">
      <div className="tour-titlebar"><span className="window-dots"><i /><i /><i /></span><span><LockKeyhole size={12} /> Runory / production</span><span className="tour-title-end">SSH</span></div>
      <div className="tour-shell">
        <aside className="tour-rail" aria-hidden="true"><BrandMark className="size-7" /><Server className="rail-selected" /><Folder /><Activity /><Box /><span /><Settings2 /><CircleHelp /></aside>
        <aside className="tour-explorer"><div className="explorer-heading">{t.sidebar}<Search size={15} /></div><p className="explorer-group"><ChevronDown size={13} />{t.group}<span>3</span></p>{["api-production", "web-production", "db-primary"].map((host, index) => <div key={host} className={`tour-host ${index === 0 ? "selected" : ""}`}><Server size={15} /><div>{host}<small>deploy@10.0.0.{index + 10}</small></div><span className="live-dot" /></div>)}<div className="explorer-bottom"><LockKeyhole size={13} />{t.secure}</div></aside>
        <div className="tour-main">
          <div className="tour-session"><span className="live-dot" /> api-production <X size={12} /><span>＋</span></div>
          <div className="tour-tabs" role="tablist" aria-label={t.label}>{t.tabs.map((label, index) => { const Icon = icons[index]; return <button key={label} ref={el => { tabs.current[index] = el; }} id={`tour-tab-${index}`} type="button" role="tab" aria-selected={active === index} aria-controls={`tour-panel-${index}`} tabIndex={active === index ? 0 : -1} onKeyDown={event => navigate(event, index)} onClick={() => setActive(index)}><Icon size={15} />{label}</button>; })}</div>
          <div id={`tour-panel-${active}`} className="tour-panel" role="tabpanel" aria-labelledby={`tour-tab-${active}`} tabIndex={0}>
            {active === 0 && <div className="tour-terminal"><p className="terminal-muted">Last login: Mon Sep 7 09:41:02 2026</p><p><b>deploy@api-production</b><span>:~$</span> uptime</p><pre>{"09:42:18 up 12 days, 3:21, 2 users\nload average: 0.12, 0.08, 0.05"}</pre><p><b>deploy@api-production</b><span>:~$</span> docker ps</p><pre>{"NAME          STATUS          PORTS\napi-server    Up 12 days      0.0.0.0:3000\nnginx         Up 12 days      0.0.0.0:80\nredis         Up 12 days      6379/tcp"}</pre><p><b>deploy@api-production</b><span>:~$</span><i className="terminal-cursor" /></p><div className="terminal-decoration" aria-hidden="true"><Terminal /><span>SSH</span></div></div>}
            {active === 1 && <div className="tour-files"><div className="tour-path"><Folder size={16} /> /home/deploy/api</div><div className="file-row file-head"><span>{t.name}</span><span>{t.size}</span></div>{files.map(([name, size], index) => <div className="file-row" key={name}><span>{index < 3 ? <Folder size={16} /> : <FileCode2 size={16} />}{name}</span><span>{size}</span></div>)}<div className="transfer-status"><ArrowDownToLine size={16} /><span>README.md<small>{t.transferred}</small></span><Check size={16} /></div></div>}
            {active === 2 && <div className="tour-monitor"><div className="monitor-heading"><h3>{t.dashboard}</h3><span className="live-dot" /></div><p>{t.agentless}</p><div className="metric-grid">{[12, 41, 28].map((value, index) => <div className="metric" key={value}><span>{t.metrics[index]}</span><strong>{value}<small>%</small></strong><div className="metric-bars" aria-hidden="true">{[20, 35, 24, 42, 30, 52, 37, 43, 29, 33, 21, 38].map((height, i) => <i key={i} style={{ height: `${height + index * 7}%` }} />)}</div></div>)}</div><h4>{t.services}</h4>{["nginx", "docker", "redis"].map(name => <div className="service-row" key={name}><span><Box size={14} />{name}</span><span><span className="live-dot" />{t.running}</span></div>)}</div>}
            {active === 3 && <div className="tour-deploy"><div className="deploy-heading"><div><span>{t.deployments}</span><h3>{t.deploymentName}</h3></div><span className="deploy-badge"><Check size={13} />{t.deployed}</span></div><div className="deploy-commit"><GitBranch size={15} />main<span>e8a42f1</span></div><div className="deploy-steps">{t.deploymentSteps.map((step, i) => <div key={step}><span><Check size={14} /></span><p>{step}</p><small>0{i + 1}</small></div>)}</div><p className="deploy-note">{t.deployNote}</p></div>}
          </div>
          <div className="tour-statusbar"><span><span className="live-dot" />{t.connected}</span><span>UTF-8 <span>•</span> bash</span></div>
        </div>
        <aside className="tour-inspector"><p>{t.detail}</p><div className="inspector-server"><Server size={25} /><strong>api-production</strong><span><span className="live-dot" />{t.connected}</span></div><dl><dt>{t.host}</dt><dd>10.0.0.10:22</dd><dt>{t.auth}</dt><dd>{t.key}</dd></dl><div className="inspector-note"><LayoutGrid size={20} /><p>{t.context}</p></div><HardDrive className="inspector-drive" size={45} strokeWidth={1} /></aside>
      </div>
    </div>
    <p className="tour-hint"><span>↑</span>{t.hint}</p>
  </div>;
}
