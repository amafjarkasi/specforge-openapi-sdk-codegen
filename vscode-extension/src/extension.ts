import * as vscode from 'vscode';
import { execFile, ChildProcess } from 'child_process';
import * as path from 'path';
import * as fs from 'fs';

// ---------------------------------------------------------------------------
// Globals
// ---------------------------------------------------------------------------

let outputChannel: vscode.OutputChannel;
let statusBarItem: vscode.StatusBarItem;
let mockServerProcess: ChildProcess | undefined;

const SPEC_PATTERNS = ['openapi.yaml', 'openapi.json', 'swagger.yaml', 'swagger.json'];

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function getConfig(): vscode.WorkspaceConfiguration {
    return vscode.workspace.getConfiguration('specforge');
}

function getSpecforgePath(): string {
    return getConfig().get<string>('binaryPath', 'specforge');
}

function getLogLevel(): string {
    return getConfig().get<string>('logLevel', 'info');
}

/**
 * Search the workspace for a common OpenAPI spec file. If the active editor
 * has an open YAML/JSON file, prefer that.
 */
function findOpenApiSpec(): string | undefined {
    // Prefer the active editor if it looks like a spec file.
    const editor = vscode.window.activeTextEditor;
    if (editor) {
        const doc = editor.document;
        const ext = path.extname(doc.fileName).toLowerCase();
        if (ext === '.yaml' || ext === '.json') {
            const base = path.basename(doc.fileName).toLowerCase();
            if (base === 'openapi.yaml' || base === 'openapi.json' ||
                base === 'swagger.yaml' || base === 'swagger.json') {
                return doc.fileName;
            }
        }
    }

    const workspaceFolders = vscode.workspace.workspaceFolders;
    if (!workspaceFolders) return undefined;

    for (const name of SPEC_PATTERNS) {
        for (const folder of workspaceFolders) {
            const uri = vscode.Uri.joinPath(folder.uri, name);
            try {
                const stat = fs.statSync(uri.fsPath);
                if (stat.isFile()) {
                    return uri.fsPath;
                }
            } catch { /* not found */ }
        }
    }
    return undefined;
}

/**
 * Show a quick-pick for the user to choose a target language.
 */
async function pickLanguage(): Promise<string | undefined> {
    const defaultLang = getConfig().get<string>('defaultLang', 'ts');
    const items: vscode.QuickPickItem[] = [
        { label: 'TypeScript', description: 'ts', picked: defaultLang === 'ts' },
        { label: 'Go', description: 'go', picked: defaultLang === 'go' },
        { label: 'Rust', description: 'rust', picked: defaultLang === 'rust' },
    ];
    const picked = await vscode.window.showQuickPick(items, {
        placeHolder: 'Select target language',
        title: 'specforge - Target Language',
    });
    return picked?.description;
}

/**
 * Show a quick-pick for the user to choose an OpenAPI version.
 */
async function pickOpenApiVersion(): Promise<string | undefined> {
    const items: vscode.QuickPickItem[] = [
        { label: 'OpenAPI 3.1', description: '3.1', picked: true },
        { label: 'OpenAPI 3.0', description: '3.0' },
    ];
    const picked = await vscode.window.showQuickPick(items, {
        placeHolder: 'Select target OpenAPI version',
        title: 'specforge - Convert Version',
    });
    return picked?.description;
}

/**
 * Pick one or more files from the workspace.
 */
async function pickFiles(title: string, multi = false): Promise<string[] | undefined> {
    const result = await vscode.window.showOpenDialog({
        canSelectFiles: true,
        canSelectFolders: false,
        canSelectMany: multi,
        openLabel: 'Select',
        title,
        filters: {
            'OpenAPI Specs': ['yaml', 'json'],
            'All Files': ['*'],
        },
    });
    return result?.map((uri) => uri.fsPath);
}

/**
 * Pick an output file path.
 */
async function pickOutputFile(defaultName: string): Promise<string | undefined> {
    const result = await vscode.window.showSaveDialog({
        defaultUri: vscode.Uri.file(defaultName),
        title: 'specforge - Save Output',
        filters: {
            'YAML': ['yaml'],
            'JSON': ['json'],
            'Markdown': ['md'],
            'All Files': ['*'],
        },
    });
    return result?.fsPath;
}

/**
 * Run a specforge CLI command with progress, output capture, and error handling.
 */
async function runSpecforge(
    args: string[],
    options: {
        title: string;
        cwd?: string;
        /** If true, show output in a new read-only editor tab instead of the output channel. */
        showAsDocument?: boolean;
        documentLanguage?: string;
        /** If set, write output to this file instead of stdout. */
        outputFile?: string;
    }
): Promise<{ stdout: string; stderr: string; exitCode: number }> {
    const cwd = options.cwd || vscode.workspace.workspaceFolders?.[0]?.uri.fsPath;
    const binary = getSpecforgePath();
    const logLevel = getLogLevel();

    // Insert --log-level if not already present.
    if (!args.includes('--log-level') && !args.includes('-v')) {
        args.push('--log-level', logLevel);
    }

    outputChannel.clear();
    outputChannel.appendLine(`> ${binary} ${args.join(' ')}`);
    outputChannel.show(true);

    return vscode.window.withProgress(
        {
            location: vscode.ProgressLocation.Notification,
            title: options.title,
            cancellable: false,
        },
        async (progress) => {
            return new Promise<{ stdout: string; stderr: string; exitCode: number }>((resolve) => {
                const proc = execFile(binary, args, { cwd, maxBuffer: 10 * 1024 * 1024 }, async (error, stdout, stderr) => {
                    const exitCode = error ? (error as any).code || 1 : 0;

                    if (stdout) {
                        outputChannel.appendLine(stdout);
                    }
                    if (stderr) {
                        outputChannel.appendLine(stderr);
                    }

                    if (exitCode !== 0 && !stdout) {
                        vscode.window.showErrorMessage(
                            `${options.title} failed: ${stderr || error?.message || 'unknown error'}`
                        );
                    }

                    // If showAsDocument, open the stdout in a new editor.
                    if (options.showAsDocument && stdout) {
                        const doc = await vscode.workspace.openTextDocument({
                            content: stdout,
                            language: options.documentLanguage || 'json',
                        });
                        vscode.window.showTextDocument(doc, { preview: true });
                    }

                    // If outputFile, write the output.
                    if (options.outputFile && stdout) {
                        try {
                            fs.writeFileSync(options.outputFile, stdout, 'utf-8');
                            vscode.window.showInformationMessage(`Written to ${options.outputFile}`);
                        } catch (writeErr: any) {
                            vscode.window.showErrorMessage(`Failed to write file: ${writeErr.message}`);
                        }
                    }

                    resolve({ stdout, stderr, exitCode });
                });

                if (!proc) {
                    resolve({ stdout: '', stderr: 'Failed to spawn process', exitCode: 1 });
                }
            });
        }
    );
}

// ---------------------------------------------------------------------------
// Command implementations
// ---------------------------------------------------------------------------

async function cmdGenerate(): Promise<void> {
    const spec = findOpenApiSpec();
    if (!spec) {
        vscode.window.showErrorMessage('No OpenAPI spec found. Open a spec file or place one in your workspace root.');
        return;
    }
    const lang = await pickLanguage();
    if (!lang) return;

    const config = getConfig();
    const outDir = await vscode.window.showInputBox({
        prompt: 'Output directory for the generated SDK',
        value: config.get('outputDir', './generated'),
        title: 'specforge - Output Directory',
    });
    if (outDir === undefined) return;

    const pkgName = await vscode.window.showInputBox({
        prompt: 'Package/module name (optional, leave empty for default)',
        title: 'specforge - Package Name',
    });

    const args = ['generate', spec, '-o', outDir, '-l', lang];
    if (pkgName) args.push('-n', pkgName);

    const result = await runSpecforge(args, { title: 'Generating SDK' });
    if (result.exitCode === 0) {
        vscode.window.showInformationMessage(`SDK generated (${lang}) to ${outDir}`);
    }
}

async function cmdCheck(): Promise<void> {
    const spec = findOpenApiSpec();
    if (!spec) {
        vscode.window.showErrorMessage('No OpenAPI spec found.');
        return;
    }

    const strict = await vscode.window.showQuickPick(
        [
            { label: 'Normal', description: 'Report errors and warnings', picked: true },
            { label: 'Strict', description: 'Treat warnings as errors' },
        ],
        { placeHolder: 'Validation mode', title: 'specforge - Check Mode' }
    );
    if (!strict) return;

    const args = ['check', spec];
    if (strict.description?.includes('Strict')) args.push('--strict');

    const result = await runSpecforge(args, { title: 'Linting & Validating Spec' });
    if (result.exitCode === 0) {
        vscode.window.showInformationMessage('Spec is valid');
    } else {
        vscode.window.showWarningMessage('Spec has validation issues. See output for details.');
    }
}

async function cmdDiff(): Promise<void> {
    const files = await pickFiles('specforge - Select two spec files to compare', true);
    if (!files || files.length < 2) {
        if (files && files.length === 1) {
            vscode.window.showWarningMessage('Please select exactly two spec files to diff.');
        }
        return;
    }

    const args = ['diff', files[0], files[1]];
    const result = await runSpecforge(args, {
        title: 'Comparing Specs',
        showAsDocument: true,
        documentLanguage: 'plaintext',
    });

    if (result.exitCode === 0) {
        vscode.window.showInformationMessage('No breaking changes found.');
    } else {
        vscode.window.showWarningMessage('Breaking changes detected. See diff output.');
    }
}

async function cmdEmit(): Promise<void> {
    const spec = findOpenApiSpec();
    if (!spec) {
        vscode.window.showErrorMessage('No OpenAPI spec found.');
        return;
    }

    const result = await runSpecforge(['emit', spec], {
        title: 'Emitting IR',
        showAsDocument: true,
        documentLanguage: 'json',
    });

    if (result.exitCode !== 0) {
        vscode.window.showErrorMessage('Failed to emit IR. See output.');
    }
}

async function cmdInit(): Promise<void> {
    const outDir = await vscode.window.showInputBox({
        prompt: 'Output directory for the scaffolded spec',
        value: '.',
        title: 'specforge - Init Output Directory',
    });
    if (outDir === undefined) return;

    const title = await vscode.window.showInputBox({
        prompt: 'API title',
        value: 'My API',
        title: 'specforge - API Title',
    });
    if (title === undefined) return;

    const version = await vscode.window.showInputBox({
        prompt: 'API version',
        value: '1.0.0',
        title: 'specforge - API Version',
    });
    if (version === undefined) return;

    const args = ['init', '--out', outDir, '--title', title, '--version', version];
    await runSpecforge(args, { title: 'Scaffolding New Spec' });
    vscode.window.showInformationMessage(`Scaffolded spec in ${outDir}`);
}

async function cmdConvert(): Promise<void> {
    const spec = findOpenApiSpec();
    if (!spec) {
        vscode.window.showErrorMessage('No OpenAPI spec found.');
        return;
    }

    const targetVersion = await pickOpenApiVersion();
    if (!targetVersion) return;

    const args = ['convert', spec, '--to', targetVersion];
    const result = await runSpecforge(args, {
        title: `Converting to OpenAPI ${targetVersion}`,
        showAsDocument: true,
        documentLanguage: spec.endsWith('.json') ? 'json' : 'yaml',
    });

    if (result.exitCode === 0) {
        const action = await vscode.window.showInformationMessage(
            `Converted to OpenAPI ${targetVersion}. Save to file?`,
            'Save',
            'Discard'
        );
        if (action === 'Save') {
            const outputFile = await pickOutputFile(path.basename(spec));
            if (outputFile && result.stdout) {
                fs.writeFileSync(outputFile, result.stdout, 'utf-8');
                vscode.window.showInformationMessage(`Saved to ${outputFile}`);
            }
        }
    }
}

async function cmdMerge(): Promise<void> {
    const files = await pickFiles('specforge - Select specs to merge', true);
    if (!files || files.length < 2) {
        vscode.window.showWarningMessage('Select at least two spec files to merge.');
        return;
    }

    const format = await vscode.window.showQuickPick(
        [
            { label: 'YAML', description: 'yaml', picked: true },
            { label: 'JSON', description: 'json' },
        ],
        { placeHolder: 'Output format', title: 'specforge - Merge Format' }
    );
    if (!format) return;

    const args = ['merge', ...files, '--format', format.description!];
    const result = await runSpecforge(args, {
        title: 'Merging Specs',
        showAsDocument: true,
        documentLanguage: format.description === 'json' ? 'json' : 'yaml',
    });

    if (result.exitCode === 0) {
        const action = await vscode.window.showInformationMessage(
            'Merge complete. Save to file?',
            'Save',
            'Discard'
        );
        if (action === 'Save') {
            const ext = format.description === 'json' ? '.json' : '.yaml';
            const outputFile = await pickOutputFile(`merged${ext}`);
            if (outputFile && result.stdout) {
                fs.writeFileSync(outputFile, result.stdout, 'utf-8');
                vscode.window.showInformationMessage(`Saved to ${outputFile}`);
            }
        }
    }
}

async function cmdMigrate(): Promise<void> {
    const files = await pickFiles('specforge - Select old and new spec files', true);
    if (!files || files.length < 2) {
        vscode.window.showWarningMessage('Select exactly two spec files (old, new) to generate a migration guide.');
        return;
    }

    const args = ['migrate', files[0], files[1]];
    const result = await runSpecforge(args, {
        title: 'Generating Migration Guide',
        showAsDocument: true,
        documentLanguage: 'markdown',
    });

    if (result.exitCode === 0) {
        const action = await vscode.window.showInformationMessage(
            'Migration guide generated. Save to file?',
            'Save',
            'Discard'
        );
        if (action === 'Save') {
            const outputFile = await pickOutputFile('MIGRATION.md');
            if (outputFile && result.stdout) {
                fs.writeFileSync(outputFile, result.stdout, 'utf-8');
                vscode.window.showInformationMessage(`Saved to ${outputFile}`);
            }
        }
    }
}

async function cmdDocs(): Promise<void> {
    const spec = findOpenApiSpec();
    if (!spec) {
        vscode.window.showErrorMessage('No OpenAPI spec found.');
        return;
    }

    const outDir = await vscode.window.showInputBox({
        prompt: 'Output directory for documentation',
        value: './docs',
        title: 'specforge - Docs Output Directory',
    });
    if (outDir === undefined) return;

    await runSpecforge(['docs', spec, '-o', outDir], { title: 'Generating Documentation' });
    vscode.window.showInformationMessage(`Documentation generated in ${outDir}`);
}

async function cmdTest(): Promise<void> {
    const spec = findOpenApiSpec();
    if (!spec) {
        vscode.window.showErrorMessage('No OpenAPI spec found.');
        return;
    }

    const lang = await pickLanguage();
    if (!lang) return;

    const outDir = await vscode.window.showInputBox({
        prompt: 'Output directory for test files',
        value: './tests',
        title: 'specforge - Test Output Directory',
    });
    if (outDir === undefined) return;

    await runSpecforge(['test', spec, '-o', outDir, '-l', lang], { title: 'Generating Tests' });
    vscode.window.showInformationMessage(`Tests generated (${lang}) in ${outDir}`);
}

async function cmdVersions(): Promise<void> {
    const spec = findOpenApiSpec();
    if (!spec) {
        vscode.window.showErrorMessage('No OpenAPI spec found.');
        return;
    }

    await runSpecforge(['versions', spec], {
        title: 'Listing API Versions',
        showAsDocument: true,
        documentLanguage: 'plaintext',
    });
}

async function cmdWorkspace(): Promise<void> {
    const configPath = await vscode.window.showInputBox({
        prompt: 'Path to workspace config file',
        value: '.specforge-workspace.yaml',
        title: 'specforge - Workspace Config',
    });
    if (configPath === undefined) return;

    const args = ['workspace', '--config', configPath];
    await runSpecforge(args, { title: 'Generating Workspace SDKs' });
    vscode.window.showInformationMessage('Workspace SDKs generated.');
}

async function cmdWorkspaceInit(): Promise<void> {
    const dir = await vscode.window.showInputBox({
        prompt: 'Directory to scan for spec files',
        value: '.',
        title: 'specforge - Scan Directory',
    });
    if (dir === undefined) return;

    const outConfig = await vscode.window.showInputBox({
        prompt: 'Output workspace config file',
        value: '.specforge-workspace.yaml',
        title: 'specforge - Output Config File',
    });
    if (outConfig === undefined) return;

    await runSpecforge(['workspace-init', dir, '--out', outConfig], {
        title: 'Initializing Workspace Config',
    });
    vscode.window.showInformationMessage(`Workspace config written to ${outConfig}`);
}

async function cmdDashboard(): Promise<void> {
    const spec = findOpenApiSpec();
    if (!spec) {
        vscode.window.showErrorMessage('No OpenAPI spec found.');
        return;
    }

    // Open the dashboard in the default browser.
    const webUri = vscode.Uri.parse('https://specforge.dev/dashboard');
    await vscode.env.openExternal(webUri);
    vscode.window.showInformationMessage('Opening specforge dashboard in browser.');
}

async function cmdSecurity(): Promise<void> {
    const spec = findOpenApiSpec();
    if (!spec) {
        vscode.window.showErrorMessage('No OpenAPI spec found.');
        return;
    }

    const args = ['security', spec];
    const result = await runSpecforge(args, {
        title: 'Analyzing Security Requirements',
        showAsDocument: true,
        documentLanguage: 'markdown',
    });

    if (result.exitCode === 0) {
        vscode.window.showInformationMessage('Security analysis complete. See output.');
    }
}

async function cmdGraph(): Promise<void> {
    const spec = findOpenApiSpec();
    if (!spec) {
        vscode.window.showErrorMessage('No OpenAPI spec found.');
        return;
    }

    const args = ['graph', spec];
    const result = await runSpecforge(args, {
        title: 'Showing Dependency Graph',
        showAsDocument: true,
        documentLanguage: 'plaintext',
    });

    if (result.exitCode !== 0) {
        vscode.window.showErrorMessage('Failed to generate dependency graph. See output.');
    }
}

async function cmdAnalyze(): Promise<void> {
    const spec = findOpenApiSpec();
    if (!spec) {
        vscode.window.showErrorMessage('No OpenAPI spec found.');
        return;
    }

    const format = await vscode.window.showQuickPick(
        [
            { label: 'Text', description: 'text', picked: true },
            { label: 'JSON', description: 'json' },
            { label: 'Markdown', description: 'markdown' },
        ],
        { placeHolder: 'Output format', title: 'specforge - Analysis Format' }
    );
    if (!format) return;

    const args = ['analyze', spec, '--format', format.description!];
    const result = await runSpecforge(args, {
        title: 'Bundle Analysis',
        showAsDocument: true,
        documentLanguage: format.description === 'json' ? 'json' : format.description === 'markdown' ? 'markdown' : 'plaintext',
    });

    if (result.exitCode !== 0) {
        vscode.window.showErrorMessage('Analysis failed. See output.');
    }
}

async function cmdMock(): Promise<void> {
    const spec = findOpenApiSpec();
    if (!spec) {
        vscode.window.showErrorMessage('No OpenAPI spec found.');
        return;
    }

    // If a mock server is already running, stop it.
    if (mockServerProcess) {
        const stop = await vscode.window.showWarningMessage(
            'A mock server is already running. Stop it first?',
            'Stop & Restart',
            'Cancel'
        );
        if (stop === 'Stop & Restart') {
            mockServerProcess.kill();
            mockServerProcess = undefined;
            statusBarItem.hide();
        } else {
            return;
        }
    }

    const portInput = await vscode.window.showInputBox({
        prompt: 'Port (leave empty for random available port)',
        title: 'specforge - Mock Server Port',
    });
    if (portInput === undefined) return;

    const args = ['mock', spec];
    if (portInput) args.push('--port', portInput);

    outputChannel.clear();
    outputChannel.appendLine(`> ${getSpecforgePath()} ${args.join(' ')}`);
    outputChannel.show(true);

    const cwd = vscode.workspace.workspaceFolders?.[0]?.uri.fsPath;
    mockServerProcess = execFile(getSpecforgePath(), args, { cwd }, (error, stdout, stderr) => {
        if (error) {
            outputChannel.appendLine(`Mock server stopped: ${error.message}`);
        }
        if (stdout) outputChannel.appendLine(stdout);
        if (stderr) outputChannel.appendLine(stderr);
        mockServerProcess = undefined;
        statusBarItem.hide();
    });

    // Show status bar indicator.
    statusBarItem.text = '$(vm-running) Mock Server';
    statusBarItem.tooltip = 'Click to stop mock server';
    statusBarItem.command = 'specforge.stopMock';
    statusBarItem.show();

    vscode.window.showInformationMessage('Mock server starting. See output for details.');
}

async function cmdStopMock(): Promise<void> {
    if (mockServerProcess) {
        mockServerProcess.kill();
        mockServerProcess = undefined;
        statusBarItem.hide();
        vscode.window.showInformationMessage('Mock server stopped.');
    }
}

async function cmdExport(): Promise<void> {
    const spec = findOpenApiSpec();
    if (!spec) {
        vscode.window.showErrorMessage('No OpenAPI spec found.');
        return;
    }

    const args = ['export', spec, '--format', 'swagger-editor'];
    const result = await runSpecforge(args, {
        title: 'Exporting for Swagger Editor',
        showAsDocument: true,
        documentLanguage: 'json',
    });

    if (result.exitCode === 0) {
        const action = await vscode.window.showInformationMessage(
            'Export complete. Save to file?',
            'Save',
            'Discard'
        );
        if (action === 'Save') {
            const outputFile = await pickOutputFile('swagger-bundle.json');
            if (outputFile && result.stdout) {
                fs.writeFileSync(outputFile, result.stdout, 'utf-8');
                vscode.window.showInformationMessage(`Saved to ${outputFile}`);
            }
        }
    }
}

async function cmdDemo(): Promise<void> {
    const args = ['demo'];
    const result = await runSpecforge(args, {
        title: 'Generating Demo Spec',
        showAsDocument: true,
        documentLanguage: 'yaml',
    });

    if (result.exitCode === 0) {
        const action = await vscode.window.showInformationMessage(
            'Demo spec generated. Save to file?',
            'Save',
            'Discard'
        );
        if (action === 'Save') {
            const outputFile = await pickOutputFile('petstore.yaml');
            if (outputFile && result.stdout) {
                fs.writeFileSync(outputFile, result.stdout, 'utf-8');
                vscode.window.showInformationMessage(`Saved to ${outputFile}`);
            }
        }
    }
}

async function cmdEvolution(): Promise<void> {
    const spec = findOpenApiSpec();
    if (!spec) {
        vscode.window.showErrorMessage('No OpenAPI spec found.');
        return;
    }

    const format = await vscode.window.showQuickPick(
        [
            { label: 'Text', description: 'text', picked: true },
            { label: 'JSON', description: 'json' },
            { label: 'Markdown', description: 'markdown' },
        ],
        { placeHolder: 'Output format', title: 'specforge - Evolution Format' }
    );
    if (!format) return;

    const args = ['evolution', spec, '--format', format.description!];
    const result = await runSpecforge(args, {
        title: 'Tracking Schema Evolution',
        showAsDocument: true,
        documentLanguage: format.description === 'json' ? 'json' : format.description === 'markdown' ? 'markdown' : 'plaintext',
    });

    if (result.exitCode !== 0) {
        vscode.window.showErrorMessage('Failed to track evolution. See output.');
    }
}

async function cmdInfer(): Promise<void> {
    const files = await pickFiles('specforge - Select a JSON file to infer from', false);
    if (!files || files.length === 0) return;

    const name = await vscode.window.showInputBox({
        prompt: 'Schema / model name',
        value: 'Inferred',
        title: 'specforge - Schema Name',
    });
    if (name === undefined) return;

    const args = ['infer', files[0], '--name', name];
    const result = await runSpecforge(args, {
        title: 'Inferring Spec from JSON',
        showAsDocument: true,
        documentLanguage: 'json',
    });

    if (result.exitCode === 0) {
        const action = await vscode.window.showInformationMessage(
            'Spec inferred. Save to file?',
            'Save',
            'Discard'
        );
        if (action === 'Save') {
            const outputFile = await pickOutputFile('inferred-openapi.json');
            if (outputFile && result.stdout) {
                fs.writeFileSync(outputFile, result.stdout, 'utf-8');
                vscode.window.showInformationMessage(`Saved to ${outputFile}`);
            }
        }
    }
}

async function cmdVerify(): Promise<void> {
    const spec = findOpenApiSpec();
    if (!spec) {
        vscode.window.showErrorMessage('No OpenAPI spec found.');
        return;
    }

    const baseUrl = await vscode.window.showInputBox({
        prompt: 'Base URL of the running API (e.g. http://localhost:3000)',
        title: 'specforge - API Base URL',
        validateInput: (value) => {
            if (!value) return 'Base URL is required';
            try { new URL(value); return null; } catch { return 'Invalid URL'; }
        },
    });
    if (!baseUrl) return;

    const auth = await vscode.window.showInputBox({
        prompt: 'Authorization header value (optional, e.g. Bearer <token>)',
        title: 'specforge - Auth Token',
    });

    const args = ['verify', spec, '--base-url', baseUrl];
    if (auth) args.push('--auth', auth);

    const result = await runSpecforge(args, {
        title: 'Verifying Running API',
        showAsDocument: true,
        documentLanguage: 'json',
    });

    if (result.exitCode === 0) {
        vscode.window.showInformationMessage('All endpoints passed verification.');
    } else {
        vscode.window.showWarningMessage('Some endpoints failed verification. See output.');
    }
}

async function cmdChangelog(): Promise<void> {
    const spec = findOpenApiSpec();
    if (!spec) {
        vscode.window.showErrorMessage('No OpenAPI spec found.');
        return;
    }

    const args = ['changelog', spec];
    const result = await runSpecforge(args, {
        title: 'Generating Changelog',
        showAsDocument: true,
        documentLanguage: 'markdown',
    });

    if (result.exitCode === 0) {
        const action = await vscode.window.showInformationMessage(
            'Changelog generated. Save to file?',
            'Save',
            'Discard'
        );
        if (action === 'Save') {
            const outputFile = await pickOutputFile('CHANGELOG.md');
            if (outputFile && result.stdout) {
                fs.writeFileSync(outputFile, result.stdout, 'utf-8');
                vscode.window.showInformationMessage(`Saved to ${outputFile}`);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Auto-validation on save
// ---------------------------------------------------------------------------

function setupAutoValidation(context: vscode.ExtensionContext): void {
    context.subscriptions.push(
        vscode.workspace.onDidSaveTextDocument(async (document) => {
            if (!getConfig().get<boolean>('autoValidate', false)) return;

            const fileName = path.basename(document.fileName).toLowerCase();
            const isSpec = SPEC_PATTERNS.includes(fileName);
            if (!isSpec) return;

            const spec = document.fileName;
            outputChannel.appendLine(`[auto-validate] Validating ${spec}...`);

            await runSpecforge(['check', spec], {
                title: 'Auto-validating Spec',
            });
        })
    );
}

// ---------------------------------------------------------------------------
// Activation
// ---------------------------------------------------------------------------

export function activate(context: vscode.ExtensionContext) {
    // Create output channel and status bar.
    outputChannel = vscode.window.createOutputChannel('specforge');
    statusBarItem = vscode.window.createStatusBarItem(vscode.StatusBarAlignment.Left, 100);

    // Register all commands.
    const commands: [string, () => Promise<void>][] = [
        ['specforge.generate', cmdGenerate],
        ['specforge.check', cmdCheck],
        ['specforge.diff', cmdDiff],
        ['specforge.emit', cmdEmit],
        ['specforge.init', cmdInit],
        ['specforge.convert', cmdConvert],
        ['specforge.merge', cmdMerge],
        ['specforge.migrate', cmdMigrate],
        ['specforge.docs', cmdDocs],
        ['specforge.test', cmdTest],
        ['specforge.versions', cmdVersions],
        ['specforge.workspace', cmdWorkspace],
        ['specforge.workspaceInit', cmdWorkspaceInit],
        ['specforge.dashboard', cmdDashboard],
        ['specforge.security', cmdSecurity],
        ['specforge.graph', cmdGraph],
        ['specforge.analyze', cmdAnalyze],
        ['specforge.mock', cmdMock],
        ['specforge.stopMock', cmdStopMock],
        ['specforge.export', cmdExport],
        ['specforge.demo', cmdDemo],
        ['specforge.evolution', cmdEvolution],
        ['specforge.infer', cmdInfer],
        ['specforge.verify', cmdVerify],
        ['specforge.changelog', cmdChangelog],
    ];

    for (const [id, handler] of commands) {
        context.subscriptions.push(
            vscode.commands.registerCommand(id, handler)
        );
    }

    // Set up auto-validation on save.
    setupAutoValidation(context);
}

export function deactivate() {
    // Stop mock server if running.
    if (mockServerProcess) {
        mockServerProcess.kill();
        mockServerProcess = undefined;
    }
    if (statusBarItem) {
        statusBarItem.dispose();
    }
    if (outputChannel) {
        outputChannel.dispose();
    }
}
