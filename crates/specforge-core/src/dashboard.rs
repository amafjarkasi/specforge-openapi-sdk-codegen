//! SDK observability dashboard generator.
//!
//! Generates a self-contained static HTML dashboard that visualizes SDK metrics
//! exported by the TypeScript SDK's `MetricsCollector`.

use std::path::Path;

/// Generate a static HTML dashboard from a JSON metrics file.
///
/// `metrics_json` is the raw JSON string produced by the SDK's
/// `MetricsCollector.getMetrics()` (or `exportMetrics()`).
///
/// `out_dir` is the directory where `index.html` and `metrics.json` will be
/// written. The directory is created if it does not exist.
///
/// Returns the list of files written (relative to `out_dir`).
pub fn generate_dashboard(metrics_json: &str, out_dir: &Path) -> std::io::Result<Vec<String>> {
    std::fs::create_dir_all(out_dir)?;
    let metrics_path = out_dir.join("metrics.json");
    std::fs::write(&metrics_path, metrics_json)?;
    let html = render_dashboard_html();
    let index_path = out_dir.join("index.html");
    std::fs::write(&index_path, &html)?;
    Ok(vec!["index.html".to_string(), "metrics.json".to_string()])
}

fn render_dashboard_html() -> String {
    r##"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>SpecForge SDK Dashboard</title>
    <script src="https://cdn.jsdelivr.net/npm/chart.js@4/dist/chart.umd.min.js"></script>
    <style>
        :root{--bg:#0d1117;--surface:#161b22;--border:#30363d;--text:#e6edf3;--muted:#8b949e;--accent:#f97316;--green:#3fb950;--red:#f85149;--blue:#58a6ff;--purple:#bc8cff}
        *{margin:0;padding:0;box-sizing:border-box}
        body{font-family:ui-sans-serif,system-ui,-apple-system,sans-serif;background:var(--bg);color:var(--text);line-height:1.5}
        header{padding:1.5rem 2rem;border-bottom:1px solid var(--border);display:flex;align-items:center;gap:1rem}
        header h1{font-size:1.5rem;font-weight:700}
        header h1 span{color:var(--accent)}
        .status{margin-left:auto;display:flex;align-items:center;gap:0.5rem;font-size:0.85rem;color:var(--muted)}
        .status-dot{width:8px;height:8px;border-radius:50%;background:var(--green);animation:pulse 2s infinite}
        @keyframes pulse{0%,100%{opacity:1}50%{opacity:0.4}}
        main{padding:2rem;max-width:1400px;margin:0 auto}
        .cards{display:grid;grid-template-columns:repeat(auto-fit,minmax(200px,1fr));gap:1rem;margin-bottom:2rem}
        .card{background:var(--surface);border:1px solid var(--border);border-radius:10px;padding:1.25rem 1.5rem;transition:border-color .2s}
        .card:hover{border-color:var(--accent)}
        .card-label{font-size:0.8rem;text-transform:uppercase;letter-spacing:0.05em;color:var(--muted);margin-bottom:0.4rem}
        .card-value{font-size:2rem;font-weight:700;font-variant-numeric:tabular-nums}
        .card-sub{font-size:0.8rem;color:var(--muted);margin-top:0.25rem}
        .text-accent{color:var(--accent)}.text-green{color:var(--green)}.text-red{color:var(--red)}.text-blue{color:var(--blue)}.text-purple{color:var(--purple)}
        .charts{display:grid;grid-template-columns:repeat(auto-fit,minmax(420px,1fr));gap:1.5rem}
        .chart-box{background:var(--surface);border:1px solid var(--border);border-radius:10px;padding:1.25rem}
        .chart-box h3{font-size:0.9rem;font-weight:600;margin-bottom:1rem;color:var(--muted)}
        canvas{width:100%!important}
        footer{text-align:center;padding:2rem;color:var(--muted);font-size:0.8rem;border-top:1px solid var(--border);margin-top:2rem}
        footer a{color:var(--accent);text-decoration:none}
    </style>
</head>
<body>
    <header>
        <h1>&#x1f525; <span>SpecForge</span> SDK Dashboard</h1>
        <div class="status">
            <div class="status-dot" id="statusDot"></div>
            <span id="statusText">Loading...</span>
        </div>
    </header>
    <main>
        <div class="cards">
            <div class="card"><div class="card-label">Total Requests</div><div class="card-value text-accent" id="totalRequests">--</div></div>
            <div class="card"><div class="card-label">Errors</div><div class="card-value text-red" id="errorCount">--</div><div class="card-sub" id="errorRate">--</div></div>
            <div class="card"><div class="card-label">Avg Latency</div><div class="card-value text-blue" id="avgLatency">--</div><div class="card-sub">ms</div></div>
            <div class="card"><div class="card-label">Success Rate</div><div class="card-value text-green" id="successRate">--</div></div>
            <div class="card"><div class="card-label">Retries</div><div class="card-value text-purple" id="retryCount">--</div></div>
            <div class="card"><div class="card-label">Cache Hits</div><div class="card-value text-green" id="cacheHits">--</div><div class="card-sub" id="cacheRatio">--</div></div>
        </div>
        <div class="charts">
            <div class="chart-box"><h3>Request Count Over Time</h3><canvas id="requestsChart"></canvas></div>
            <div class="chart-box"><h3>Error Rate Over Time</h3><canvas id="errorsChart"></canvas></div>
            <div class="chart-box"><h3>Average Latency (ms)</h3><canvas id="latencyChart"></canvas></div>
            <div class="chart-box"><h3>Success Rate Gauge</h3><canvas id="gaugeChart"></canvas></div>
            <div class="chart-box"><h3>Cache Hit / Miss Ratio</h3><canvas id="cacheChart"></canvas></div>
            <div class="chart-box"><h3>Retry Count Over Time</h3><canvas id="retryChart"></canvas></div>
        </div>
    </main>
    <footer>Generated by <a href="https://github.com/specforge/specforge">SpecForge</a> &middot; Auto-refreshes every 5 seconds</footer>
    <script>
    const COLORS={accent:'#f97316',accentBg:'rgba(249,115,22,0.15)',green:'#3fb950',greenBg:'rgba(63,185,80,0.15)',red:'#f85149',redBg:'rgba(248,81,73,0.15)',blue:'#58a6ff',blueBg:'rgba(88,166,255,0.15)',purple:'#bc8cff',purpleBg:'rgba(188,140,255,0.15)',grid:'rgba(48,54,61,0.6)',text:'#8b949e'};
    Chart.defaults.color=COLORS.text;Chart.defaults.borderColor=COLORS.grid;
    const CO={responsive:true,maintainAspectRatio:true,animation:{duration:400},plugins:{legend:{display:false}},scales:{x:{grid:{color:COLORS.grid},ticks:{maxTicksLimit:10,color:COLORS.text}},y:{grid:{color:COLORS.grid},beginAtZero:true,ticks:{color:COLORS.text}}}};
    let rC,eC,lC,gC,cC,reC,hist=[],MH=30;
    function init(){rC=new Chart(document.getElementById('requestsChart'),{type:'line',data:{labels:[],datasets:[{label:'Requests',data:[],borderColor:COLORS.accent,backgroundColor:COLORS.accentBg,fill:true,tension:.3,pointRadius:2,borderWidth:2}]},options:CO});eC=new Chart(document.getElementById('errorsChart'),{type:'bar',data:{labels:[],datasets:[{label:'Errors',data:[],backgroundColor:COLORS.redBg,borderColor:COLORS.red,borderWidth:1,borderRadius:4}]},options:CO});lC=new Chart(document.getElementById('latencyChart'),{type:'line',data:{labels:[],datasets:[{label:'Avg Latency',data:[],borderColor:COLORS.blue,backgroundColor:COLORS.blueBg,fill:true,tension:.3,pointRadius:2,borderWidth:2}]},options:CO});gC=new Chart(document.getElementById('gaugeChart'),{type:'doughnut',data:{labels:['Success','Errors'],datasets:[{data:[100,0],backgroundColor:[COLORS.green,COLORS.red],borderWidth:0,cutout:'75%'}]},options:{responsive:true,maintainAspectRatio:true,animation:{duration:400},plugins:{legend:{display:true,position:'bottom',labels:{color:COLORS.text,padding:16}}}}});cC=new Chart(document.getElementById('cacheChart'),{type:'doughnut',data:{labels:['Hits','Misses'],datasets:[{data:[0,1],backgroundColor:[COLORS.green,COLORS.purple],borderWidth:0,cutout:'75%'}]},options:{responsive:true,maintainAspectRatio:true,animation:{duration:400},plugins:{legend:{display:true,position:'bottom',labels:{color:COLORS.text,padding:16}}}}});reC=new Chart(document.getElementById('retryChart'),{type:'line',data:{labels:[],datasets:[{label:'Retries',data:[],borderColor:COLORS.purple,backgroundColor:COLORS.purpleBg,fill:true,tension:.3,pointRadius:2,borderWidth:2}]},options:CO})}
    function update(m){const ts=new Date().toLocaleTimeString();document.getElementById('totalRequests').textContent=m.requestCount??'--';document.getElementById('errorCount').textContent=m.errorCount??'--';const er=m.requestCount>0?((m.errorCount/m.requestCount)*100).toFixed(1)+'%':'0%';document.getElementById('errorRate').textContent='Error rate: '+er;document.getElementById('avgLatency').textContent=(m.avgDurationMs??0).toFixed(1);const sr=m.requestCount>0?(((m.requestCount-m.errorCount)/m.requestCount)*100).toFixed(1)+'%':'100%';document.getElementById('successRate').textContent=sr;document.getElementById('retryCount').textContent=m.retryCount??'--';const ch=m.cacheHits??0,cm=m.cacheMisses??0;document.getElementById('cacheHits').textContent=ch;const tc=ch+cm;document.getElementById('cacheRatio').textContent=tc>0?'Hit ratio: '+((ch/tc)*100).toFixed(1)+'%':'No cache data';hist.push({...m,ts});if(hist.length>MH)hist.shift();const lb=hist.map(h=>h.ts);rC.data.labels=lb;rC.data.datasets[0].data=hist.map(h=>h.requestCount);rC.update();eC.data.labels=lb;eC.data.datasets[0].data=hist.map(h=>h.errorCount);eC.update();lC.data.labels=lb;lC.data.datasets[0].data=hist.map(h=>h.avgDurationMs);lC.update();const sp=m.requestCount>0?((m.requestCount-m.errorCount)/m.requestCount)*100:100;gC.data.datasets[0].data=[sp,100-sp];gC.update();cC.data.datasets[0].data=[ch,cm];cC.update();reC.data.labels=lb;reC.data.datasets[0].data=hist.map(h=>h.retryCount);reC.update();document.getElementById('statusDot').style.background=COLORS.green;document.getElementById('statusText').textContent='Live \u2022 '+ts}
    async function fetch(){try{const r=await fetch('metrics.json?t='+Date.now());if(!r.ok)throw new Error('HTTP '+r.status);update(await r.json())}catch(e){document.getElementById('statusDot').style.background=COLORS.red;document.getElementById('statusText').textContent='Error: '+e.message}}
    init();fetch();setInterval(fetch,5000);
    </script>
</body>
</html>"##.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn generate_dashboard_creates_files() {
        let dir = tempfile::tempdir().unwrap();
        let metrics = r#"{"requestCount":42,"errorCount":3,"totalDurationMs":15000,"avgDurationMs":357.1,"retryCount":1,"cacheHits":10,"cacheMisses":5}"#;
        let files = generate_dashboard(metrics, dir.path()).unwrap();
        assert!(files.contains(&"index.html".to_string()));
        assert!(files.contains(&"metrics.json".to_string()));
        let html = fs::read_to_string(dir.path().join("index.html")).unwrap();
        assert!(html.contains("chart.js"));
        assert!(html.contains("requestsChart"));
        assert!(html.contains("errorsChart"));
        assert!(html.contains("latencyChart"));
        assert!(html.contains("gaugeChart"));
        assert!(html.contains("cacheChart"));
        assert!(html.contains("retryChart"));
        assert!(html.contains("SpecForge"));
        let json = fs::read_to_string(dir.path().join("metrics.json")).unwrap();
        assert!(json.contains("requestCount"));
    }
}
