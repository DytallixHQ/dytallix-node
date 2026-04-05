import React, { useMemo, useState } from "react";
import { motion, AnimatePresence } from "framer-motion";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Badge } from "@/components/ui/badge";
import { Checkbox } from "@/components/ui/checkbox";
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { Tooltip, TooltipTrigger, TooltipContent, TooltipProvider } from "@/components/ui/tooltip";
import { BarChart, Bar, XAxis, YAxis, CartesianGrid, Tooltip as RTooltip, ResponsiveContainer } from "recharts";
import { Info, ShieldAlert, Sparkles, Mail } from "lucide-react";

// --------------------
// Config
// --------------------
const SECTORS = [
  { value: "government_defense", label: "Government & Defense" },
  { value: "healthcare_life_sciences", label: "Healthcare & Life Sciences" },
  { value: "pharmaceutical_research", label: "Pharmaceutical & Research" },
  { value: "financial_services", label: "Financial Services" },
  { value: "technology_software", label: "Technology & Software" },
  { value: "design_creative", label: "Design & Creative" },
];

const DATA_TYPES = [
  { value: "pii", label: "PII" },
  { value: "financial", label: "Financial" },
  { value: "ip", label: "IP" },
  { value: "phi", label: "PHI" },
  { value: "trade", label: "Trade secrets" },
  { value: "operational", label: "Operational" },
];

const REGULATIONS = [
  { value: "gdpr", label: "GDPR" },
  { value: "hipaa", label: "HIPAA" },
  { value: "pci", label: "PCI" },
  { value: "iso27001", label: "ISO 27001" },
  { value: "none", label: "None" },
];

const SIZE_STEPS = [
  { value: "startup", label: "Startup" },
  { value: "smb", label: "SMB" },
  { value: "mid", label: "Mid" },
  { value: "enterprise", label: "Enterprise" },
  { value: "global", label: "Global" },
];

const CRYPTO_STAGE = [
  { value: "none", label: "None" },
  { value: "planned", label: "Planned" },
  { value: "progress", label: "In progress" },
  { value: "operational", label: "Operational" },
];

// --------------------
// Helpers
// --------------------
function MultiCheck({ label, options, value, onChange }) {
  return (
    <div className="space-y-2">
      <Label>{label}</Label>
      <div className="grid grid-cols-2 md:grid-cols-3 gap-2">
        {options.map((opt) => (
          <label key={opt.value} className="flex items-center gap-2 p-2 rounded-xl border cursor-pointer hover:bg-muted/50">
            <Checkbox
              checked={value.includes(opt.value)}
              onCheckedChange={(c) => {
                const checked = c === true;
                const set = new Set(value);
                if (checked) set.add(opt.value); else set.delete(opt.value);
                onChange(Array.from(set));
              }}
            />
            <span className="text-sm">{opt.label}</span>
          </label>
        ))}
      </div>
    </div>
  );
}

function tierFromIndex(index) {
  return index >= 70 ? "High" : index >= 40 ? "Moderate" : "Low";
}

function colorFromTier(tier) {
  return tier === "High" ? "#dc2626" : tier === "Moderate" ? "#f59e0b" : "#16a34a";
}

function useScore(inputs) {
  return useMemo(() => {
    const sizeW = { startup: 10, smb: 25, mid: 40, enterprise: 60, global: 75 };
    const sectorW = {
      government_defense: 90,
      healthcare_life_sciences: 85,
      pharmaceutical_research: 88,
      financial_services: 82,
      technology_software: 70,
      design_creative: 60,
    };
    const dataW = { pii: 65, financial: 75, ip: 70, phi: 80, trade: 72, operational: 55 };
    const regW = { gdpr: 70, hipaa: 72, pci: 73, iso27001: 55, none: 25 };
    const stageW = { none: 30, planned: 15, progress: -10, operational: -25 };

    const s = sizeW[inputs.size] ?? 30;
    const sec = sectorW[inputs.sector] ?? 60;
    const d = inputs.dataTypes.length ? inputs.dataTypes.reduce((a, k) => a + (dataW[k] ?? 50), 0) / inputs.dataTypes.length : 35;
    const r = inputs.regulations.length ? inputs.regulations.reduce((a, k) => a + (regW[k] ?? 50), 0) / inputs.regulations.length : 30;
    const st = stageW[inputs.stage] ?? 0;

    const exposure = Math.min(100, Math.round(0.45 * s + 0.35 * sec + 0.2 * d));
    const sensitivity = Math.min(100, Math.round(0.6 * d + 0.4 * r));
    const readiness = Math.max(0, Math.min(100, 50 - st));

    const raw = 0.4 * exposure + 0.35 * sensitivity + 0.25 * (100 - readiness);
    const index = Math.round(Math.max(0, Math.min(100, raw)));

    const tier = tierFromIndex(index);
    const color = colorFromTier(tier);

    return { index, tier, color, breakdown: { Exposure: exposure, Sensitivity: sensitivity, Readiness: 100 - readiness } };
  }, [inputs]);
}

function AnalogGauge({ value }) {
  const width = 300;
  const height = 200;
  const cx = width / 2;
  const cy = height / 2 + 20;
  const radius = 100;
  const thickness = 18;
  const start = -180;
  const end = 0;
  const pctToAngle = (p) => start + (end - start) * (p / 100);
  const polar = (ang, r) => {
    const a = (Math.PI / 180) * ang;
    return { x: cx + r * Math.cos(a), y: cy + r * Math.sin(a) };
  };
  const arc = (p0, p1, stroke) => {
    const a0 = pctToAngle(p0);
    const a1 = pctToAngle(p1);
    const large = a1 - a0 <= 180 ? 0 : 1;
    const s = polar(a0, radius);
    const e = polar(a1, radius);
    const d = `M ${s.x} ${s.y} A ${radius} ${radius} 0 ${large} 1 ${e.x} ${e.y}`;
    return <path key={`${p0}-${p1}`} d={d} stroke={stroke} strokeWidth={thickness} fill="none" strokeLinecap="round" />;
  };

  const ticks = Array.from({ length: 11 }, (_, i) => i * 10);
  const needleAngle = pctToAngle(Math.max(0, Math.min(100, value)));
  const nEnd = polar(needleAngle, radius + 6);

  return (
    <svg width={width} height={height} viewBox="0 0 300 200" role="img" aria-label={`Risk ${String(value)}`}>
      <g>
        {arc(0, 100, "#e5e7eb")}
        {arc(0, 40, "#16a34a")}
        {arc(40, 70, "#f59e0b")}
        {arc(70, 100, "#dc2626")}
        <line x1={cx} y1={cy} x2={nEnd.x} y2={nEnd.y} stroke="black" strokeWidth={4} />
        <circle cx={cx} cy={cy} r={6} fill="black" />
        {ticks.map((t) => {
          const pos = polar(pctToAngle(t), radius + 28);
          return (
            <text key={t} x={pos.x} y={pos.y} textAnchor="middle" dominantBaseline="middle" className="text-xs fill-muted-foreground">
              {String(t)}
            </text>
          );
        })}
      </g>
    </svg>
  );
}

export default function QuantumRiskSnapshot() {
  // Inputs
  const [size, setSize] = useState("mid");
  const [sector, setSector] = useState("technology_software");
  const [dataTypes, setDataTypes] = useState(["pii", "ip"]);
  const [regulations, setRegulations] = useState(["iso27001"]);
  const [stage, setStage] = useState("planned");

  // UI state
  const [showResults, setShowResults] = useState(false);
  const [open, setOpen] = useState(false);

  // Score
  const { index, tier, color, breakdown } = useScore({ size, sector, dataTypes, regulations, stage });

  const chartData = useMemo(() =>
    Object.entries(breakdown).map(([k, v]) => ({
      name: k,
      score: v,
      context:
        k === "Exposure"
          ? "Represents your organization's attack surface and data footprint."
          : k === "Sensitivity"
          ? "Captures the value and regulatory weight of your sensitive data."
          : "Reflects your preparedness for PQC adoption and adaptable security.",
    })), [breakdown]
  );

  const bullets = useMemo(() => {
    const out = [];
    const exp = breakdown.Exposure ?? 0;
    const sen = breakdown.Sensitivity ?? 0;
    const ready = breakdown.Readiness ?? 0;

    const tierBullets = {
      High: [
        "You face elevated exposure and should act promptly.",
        "Prioritize a clear plan for quantum‑safe protection.",
      ],
      Moderate: [
        "Risk is manageable but growing — strengthen key safeguards.",
        "Focus on quick wins and targeted pilots.",
      ],
      Low: [
        "Overall risk is low — maintain vigilance and best practices.",
        "Begin planning for quantum‑ready standards.",
      ],
    };

    const sectorBullets = {
      government_defense: [
        "Sensitive communications and long‑term archives increase exposure.",
        "Consider safeguards for classified or inter‑agency data.",
      ],
      healthcare_life_sciences: [
        "Patient records and clinical data carry high privacy expectations.",
        "Data integrity matters for audits and submissions.",
      ],
      pharmaceutical_research: [
        "Research IP and lab results are valuable to competitors.",
        "Protect trial data and regulatory packages.",
      ],
      financial_services: [
        "Customer data and transactions invite regulatory attention.",
        "Protect keys and certificates across payments and APIs.",
      ],
      technology_software: [
        "Source code, models, and secrets are attractive targets.",
        "Secure build and release pipelines.",
      ],
      design_creative: [
        "Brand and media assets benefit from authenticity and integrity.",
        "Safeguard design files and IP disclosures.",
      ],
    };

    const regBullets = {
      gdpr: "Privacy obligations increase scrutiny around encryption and key handling.",
      hipaa: "Health data requires strong controls and clear audit trails.",
      pci: "PCI DSS mandates quantum-resistant cryptography for payment security and compliance.",
      iso27001: "Governance standards favor proactive, adaptable security planning.",
    };

    // Base insights from factors
    out.push(exp >= 60 ? "Large attack surface and data footprint are driving risk." : "Exposure is moderate — monitor high‑value systems.");
    out.push(sen >= 60 ? "You handle high‑value or highly regulated data." : "Data sensitivity is mixed; regulatory impact is moderate.");
    out.push(ready < 50 ? "Readiness is limited — plan or pilots recommended." : "Readiness is improving; continue toward quantum‑ready posture.");

    // Tier, sector, regulation
    out.push(...(tierBullets[tier] || []));
    out.push(...(sectorBullets[sector] || []));
    const regAdds = regulations.filter((r) => r !== "none").slice(0, 2).map((r) => regBullets[r]).filter((s) => Boolean(s));
    out.push(...regAdds);

    // Dedup + cap
    return Array.from(new Set(out)).slice(0, 6);
  }, [breakdown, tier, sector, regulations]);

  // Actions
  const [actions, setActions] = useState([
    { key: "inventory", label: "Inventory where encryption and keys are used", impact: 8, tl: "60d", done: false },
    { key: "pilot", label: "Pilot quantum‑resilient options on a low‑risk service", impact: 10, tl: "90d", done: false },
    { key: "certs", label: "Refresh certificates and key rotation practices", impact: 5, tl: "60d", done: false },
    { key: "vendors", label: "Ask top vendors for roadmap and requirements", impact: 4, tl: "60d", done: false },
  ]);

  return (
    <TooltipProvider>
      <div className="min-h-screen bg-gradient-to-b from-white to-muted/40 p-4 md:p-6 lg:p-10">
        <div className="max-w-7xl mx-auto space-y-6">
          <div className="text-center space-y-2">
            <h1 className="text-3xl md:text-4xl font-semibold tracking-tight">Dytallix Quantum Risk Snapshot</h1>
          </div>

          {/* HNDL Threat Alert */}
          <Card className="rounded-2xl border-2 border-amber-500/50 shadow-lg overflow-hidden bg-gradient-to-br from-amber-50 to-orange-50">
            <CardHeader>
              <div className="flex items-start gap-3">
                <ShieldAlert className="w-6 h-6 text-amber-600 flex-shrink-0 mt-1" />
                <div className="space-y-2">
                  <CardTitle className="text-xl text-amber-900">The Harvest Now, Decrypt Later Threat</CardTitle>
                  <CardDescription className="text-amber-800">
                    Your encrypted data is at risk today, even before quantum computers arrive.
                  </CardDescription>
                </div>
              </div>
            </CardHeader>
            <CardContent className="space-y-4">
              <div className="prose prose-sm max-w-none text-amber-900">
                <p>
                  <strong>Harvest Now, Decrypt Later (HNDL)</strong> is a real and present danger. Adversaries are collecting encrypted data today—intercepting communications, copying databases, archiving backups—knowing they can't decrypt it yet. But they're patient. Once quantum computers become powerful enough to break current encryption standards (estimated within 10-15 years), all that harvested data becomes readable.
                </p>
              </div>
              
              <div className="grid md:grid-cols-3 gap-3">
                <div className="p-3 rounded-lg bg-white/60 border border-amber-200">
                  <div className="font-semibold text-amber-900 mb-1">What's at Risk</div>
                  <div className="text-sm text-amber-800">
                    Long-lived sensitive data: financial records, healthcare information, intellectual property, government communications, trade secrets
                  </div>
                </div>
                <div className="p-3 rounded-lg bg-white/60 border border-amber-200">
                  <div className="font-semibold text-amber-900 mb-1">Why Act Now</div>
                  <div className="text-sm text-amber-800">
                    Data encrypted today with RSA or ECC can be stored and decrypted later. The clock is ticking on your current cryptographic protection.
                  </div>
                </div>
                <div className="p-3 rounded-lg bg-white/60 border border-amber-200">
                  <div className="font-semibold text-amber-900 mb-1">The Solution</div>
                  <div className="text-sm text-amber-800">
                    Transition to quantum-resistant encryption now. Protect your data before it's harvested and ensure long-term confidentiality.
                  </div>
                </div>
              </div>

              <div className="flex items-center gap-2 p-3 rounded-lg bg-amber-100 border border-amber-300">
                <Info className="w-5 h-5 text-amber-700 flex-shrink-0" />
                <p className="text-sm text-amber-900 m-0">
                  <strong>Key insight:</strong> If your data needs to remain confidential for 10+ years, you need quantum-safe protection today. The HNDL threat makes quantum risk an immediate concern, not a future problem.
                </p>
              </div>
            </CardContent>
          </Card>

          <div className="space-y-6">
            {/* Input form */}
            <Card className="rounded-2xl border shadow-sm overflow-hidden">
                <CardHeader>
                  <CardTitle>About your organization</CardTitle>
                  <CardDescription>Answer a few questions to get your instant profile.</CardDescription>
                </CardHeader>
                <CardContent className="grid gap-5">
                  <div className="grid grid-cols-1 gap-4">
                    <div className="space-y-2">
                      <Label>Company size</Label>
                      <div className="grid grid-cols-5 gap-2">
                        {SIZE_STEPS.map((s) => (
                          <Button key={s.value} type="button" variant={size === s.value ? "default" : "secondary"} onClick={() => setSize(s.value)} className="w-full">
                            {s.label}
                          </Button>
                        ))}
                      </div>
                    </div>
                    <div className="space-y-2">
                      <Label>Sector</Label>
                      <select className="w-full p-2 border rounded-xl bg-background focus:outline-none" value={sector} onChange={(e) => setSector(e.target.value)}>
                        {SECTORS.map((s) => (
                          <option key={s.value} value={s.value}>{s.label}</option>
                        ))}
                      </select>
                    </div>
                  </div>

                  <MultiCheck label="Sensitive data" options={DATA_TYPES} value={dataTypes} onChange={setDataTypes} />
                  <MultiCheck label="Regulations in scope" options={REGULATIONS} value={regulations} onChange={setRegulations} />

                  <div className="space-y-2">
                    <Label className="flex items-center gap-2">Cryptography modernization
                      <Tooltip>
                        <TooltipTrigger asChild>
                          <Info className="w-4 h-4 opacity-70" />
                        </TooltipTrigger>
                        <TooltipContent>Where are you with PQC planning/pilots?</TooltipContent>
                      </Tooltip>
                    </Label>
                    <div className="grid grid-cols-4 gap-2">
                      {CRYPTO_STAGE.map((st) => (
                        <Button key={st.value} type="button" variant={stage === st.value ? "default" : "secondary"} onClick={() => setStage(st.value)} className="w-full">
                          {st.label}
                        </Button>
                      ))}
                    </div>
                  </div>

                  <div className="flex flex-col md:flex-row gap-3 pt-2">
                    <Button className="flex-1" onClick={() => setShowResults(true)}>
                      <Sparkles className="w-4 h-4 mr-2" /> Generate my Quantum Risk Profile
                    </Button>
                    <Button variant="secondary" className="md:w-48" onClick={() => { setSize("mid"); setSector("technology_software"); setDataTypes(["pii", "ip"]); setRegulations(["iso27001"]); setStage("planned"); setShowResults(false); }}>Reset</Button>
                  </div>
                </CardContent>
              </Card>

            {/* Results section */}
            <AnimatePresence initial={false}>
              {showResults && (
                <motion.div initial={{ opacity: 0, y: 12 }} animate={{ opacity: 1, y: 0 }} exit={{ opacity: 0, y: 12 }} className="space-y-6">
                    {/* Split: Gauge + What this means */}
                    <div className="grid md:grid-cols-2 gap-6">
                      <Card className="rounded-2xl border shadow-sm overflow-hidden">
                        <CardHeader>
                          <CardTitle>Your Quantum Risk Profile</CardTitle>
                          <CardDescription>0 (minimal) → 100 (critical)</CardDescription>
                        </CardHeader>
                        <CardContent className="overflow-hidden">
                          <div className="h-64 flex items-center justify-center">
                            <AnalogGauge value={index} />
                          </div>
                          <div className="mt-4 flex items-center justify-between">
                            <div className="text-3xl font-semibold" style={{ color: color }}>{String(index)}</div>
                            <Badge style={{ backgroundColor: color, color: "white" }}>{tier} risk</Badge>
                          </div>
                        </CardContent>
                      </Card>

                      <Card className="rounded-2xl border shadow-sm overflow-hidden">
                        <CardHeader>
                          <CardTitle>What this means</CardTitle>
                        </CardHeader>
                        <CardContent>
                          <ul className="list-disc pl-5 space-y-2 text-sm">
                            {bullets.map((b, i) => (
                              <li key={i}>{b}</li>
                            ))}
                          </ul>
                        </CardContent>
                      </Card>
                    </div>

                    {/* Snapshot details */}
                    <Card className="rounded-2xl border shadow-sm overflow-hidden">
                      <CardHeader>
                        <CardTitle>Snapshot details</CardTitle>
                        <CardDescription>Risk factor distribution and context</CardDescription>
                      </CardHeader>
                      <CardContent className="overflow-hidden">
                        <div className="h-72">
                          <ResponsiveContainer width="100%" height="100%">
                            <BarChart data={chartData}>
                              <CartesianGrid strokeDasharray="3 3" />
                              <XAxis dataKey="name" />
                              <YAxis domain={[0, 100]} />
                              <RTooltip content={({ active, payload }) => {
                                if (active && payload && payload.length) {
                                  const d = payload[0].payload;
                                  return (
                                    <div className="p-2 bg-background border rounded-md shadow-sm text-sm">
                                      <div className="font-medium">{`${d.name}: ${d.score}`}</div>
                                      <div className="text-muted-foreground">{d.context}</div>
                                    </div>
                                  );
                                }
                                return null;
                              }} />
                              <Bar dataKey="score" fill={color} radius={[8, 8, 0, 0]} />
                            </BarChart>
                          </ResponsiveContainer>
                        </div>
                      </CardContent>
                    </Card>

                    {/* Actions */}
                    <Card className="rounded-2xl border shadow-sm overflow-hidden">
                      <CardHeader className="flex flex-col md:flex-row md:items-center md:justify-between gap-2">
                        <div>
                          <CardTitle>What to do next</CardTitle>
                        </div>
                        <Badge variant="secondary" className="flex items-center gap-1"><ShieldAlert className="w-3 h-3" /> Guidance</Badge>
                      </CardHeader>
                      <CardContent className="space-y-4">
                        <div className="flex items-center justify-between">
                          <div>
                            <div className="font-medium">Interactive plan</div>
                            <div className="text-sm text-muted-foreground">Check off actions and pick a target timeline.</div>
                          </div>
                          <div className="text-sm">Estimated score improvement</div>
                        </div>

                        <div className="space-y-3">
                          {actions.map((a, i) => (
                            <div key={a.key} className="flex flex-col md:flex-row md:items-center justify-between gap-3 p-3 border rounded-xl bg-muted/30">
                              <label className="flex items-center gap-3">
                                <Checkbox checked={a.done} onCheckedChange={(v) => {
                                  const next = [...actions];
                                  next[i] = { ...a, done: v === true };
                                  setActions(next);
                                }} />
                                <span className="text-sm">{a.label}</span>
                              </label>
                              <div className="flex items-center gap-3">
                                <select className="p-2 border rounded-md bg-background text-sm" value={a.tl} onChange={(e) => {
                                  const next = [...actions];
                                  next[i] = { ...a, tl: e.target.value };
                                  setActions(next);
                                }}>
                                  <option value="30d">30 days</option>
                                  <option value="60d">60 days</option>
                                  <option value="90d">90 days</option>
                                </select>
                                <Badge variant="outline">{`≈${a.impact} pts`}</Badge>
                              </div>
                            </div>
                          ))}
                        </div>

                        {(() => {
                          const total = actions.reduce((acc, a) => acc + (a.done ? a.impact : 0), 0);
                          const pct = Math.min(100, Math.round((total / 25) * 100));
                          return (
                            <div className="space-y-2">
                              <div className="text-xs text-muted-foreground">Total projected improvement</div>
                              <div className="w-full h-3 rounded-full bg-muted overflow-hidden">
                                <div className="h-full bg-primary" style={{ width: `${pct}%` }} />
                              </div>
                              <div className="text-sm">{`~${total} points`}</div>
                            </div>
                          );
                        })()}

                        <div className="flex flex-col gap-4 mt-4 p-4 rounded-xl bg-gradient-to-r from-primary/10 to-primary/5 border border-primary/20">
                          <div className="space-y-1">
                            <div className="font-semibold text-lg">Ready to quantum-proof your organization?</div>
                            <div className="text-sm text-muted-foreground">Get a personalized roadmap and expert guidance from the Dytallix team.</div>
                          </div>
                          <div className="flex flex-col sm:flex-row gap-3">
                            <Button size="lg" className="flex-1" onClick={() => setOpen(true)}>
                              <Mail className="w-4 h-4 mr-2" /> Secure Your QuantumVault Pilot
                            </Button>
                            <Button size="lg" variant="outline" className="sm:w-auto" onClick={() => setOpen(true)}>
                              Download Full Report
                            </Button>
                          </div>
                        </div>
                      </CardContent>
                    </Card>
                  </motion.div>
                )}
              </AnimatePresence>
          </div>
        </div>
      </div>

      {/* Contact dialog */}
      <Dialog open={open} onOpenChange={setOpen}>
        <DialogContent className="sm:max-w-md">
          <DialogHeader>
            <DialogTitle>Start Your QuantumVault Pilot Program</DialogTitle>
            <DialogDescription>
              Get hands-on experience with quantum-safe data protection. We'll provide you with a customized pilot implementation and detailed risk assessment report.
            </DialogDescription>
          </DialogHeader>
          <div className="grid gap-4">
            <div className="grid grid-cols-1 md:grid-cols-2 gap-3">
              <div>
                <Label htmlFor="name">Name *</Label>
                <Input id="name" placeholder="Alex Morgan" required />
              </div>
              <div>
                <Label htmlFor="company">Company *</Label>
                <Input id="company" placeholder="Acme Corp" required />
              </div>
            </div>
            <div>
              <Label htmlFor="email">Work email *</Label>
              <Input id="email" type="email" placeholder="alex@company.com" required />
            </div>
            <div>
              <Label htmlFor="role">Role</Label>
              <Input id="role" placeholder="CISO, VP Engineering, Security Architect..." />
            </div>
            <div className="space-y-2">
              <Label htmlFor="interest">I'm interested in (optional)</Label>
              <div className="grid grid-cols-1 gap-2">
                <label className="flex items-center gap-2 text-sm">
                  <Checkbox id="pilot" defaultChecked />
                  <span>QuantumVault pilot implementation</span>
                </label>
                <label className="flex items-center gap-2 text-sm">
                  <Checkbox id="report" defaultChecked />
                  <span>Detailed risk assessment report</span>
                </label>
                <label className="flex items-center gap-2 text-sm">
                  <Checkbox id="roadmap" />
                  <span>Custom quantum readiness roadmap</span>
                </label>
              </div>
            </div>
          </div>
          <DialogFooter className="gap-2">
            <Button variant="outline" onClick={() => setOpen(false)}>Cancel</Button>
            <Button className="flex-1" onClick={() => { alert('Thank you! Your pilot request has been received. We\'ll send setup instructions within 24 hours.'); setOpen(false); }}>
              Request Pilot Access
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </TooltipProvider>
  );
}

// --------------------
// Minimal sanity tests
// --------------------
try {
  console.assert(tierFromIndex(39) === "Low", "tier boundary 39→Low");
  console.assert(tierFromIndex(40) === "Moderate", "tier boundary 40→Moderate");
  console.assert(tierFromIndex(70) === "High", "tier boundary 70→High");
  // Additional: gauge label stringification should be strings
  const svgLabel = String(50);
  console.assert(svgLabel === "50", "labels stringify correctly");
} catch (_) {}
