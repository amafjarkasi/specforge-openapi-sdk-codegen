import * as vscode from 'vscode';
import { exec } from 'child_process';
import * as path from 'path';

function getSpecforgePath(): string {
    return vscode.workspace.getConfiguration('specforge').get('binaryPath', 'specforge');
}

function findOpenApiSpec(): string | undefined {
    const workspaceFolders = vscode.workspace.workspaceFolders;
    if (!workspaceFolders) return undefined;
    // Look for common spec file names
    for (const name of ['openapi.yaml', 'openapi.json', 'swagger.yaml', 'swagger.json']) {
        const uri = vscode.Uri.joinPath(workspaceFolders[0].uri, name);
        try {
            vscode.workspace.fs.stat(uri);
            return uri.fsPath;
        } catch { /* not found */ }
    }
    return undefined;
}

export function activate(context: vscode.ExtensionContext) {
    context.subscriptions.push(
        vscode.commands.registerCommand('specforge.generate', async () => {
            const spec = findOpenApiSpec();
            if (!spec) {
                vscode.window.showErrorMessage('No OpenAPI spec found in workspace');
                return;
            }
            const config = vscode.workspace.getConfiguration('specforge');
            const lang = config.get('defaultLang', 'ts');
            const out = config.get('outputDir', './generated');
            const cmd = `${getSpecforgePath()} generate "${spec}" -o "${out}" -l ${lang}`;
            exec(cmd, { cwd: vscode.workspace.workspaceFolders?.[0]?.uri.fsPath }, (err, stdout, stderr) => {
                if (err) {
                    vscode.window.showErrorMessage(`specforge failed: ${stderr}`);
                } else {
                    vscode.window.showInformationMessage(`SDK generated to ${out}`);
                }
            });
        }),

        vscode.commands.registerCommand('specforge.check', async () => {
            const spec = findOpenApiSpec();
            if (!spec) {
                vscode.window.showErrorMessage('No OpenAPI spec found in workspace');
                return;
            }
            const cmd = `${getSpecforgePath()} check "${spec}"`;
            exec(cmd, { cwd: vscode.workspace.workspaceFolders?.[0]?.uri.fsPath }, (err, stdout, stderr) => {
                if (err) {
                    vscode.window.showWarningMessage(`specforge check: ${stderr || stdout}`);
                } else {
                    vscode.window.showInformationMessage('Spec is valid');
                }
            });
        }),

        vscode.commands.registerCommand('specforge.preview', async () => {
            const spec = findOpenApiSpec();
            if (!spec) {
                vscode.window.showErrorMessage('No OpenAPI spec found in workspace');
                return;
            }
            const cmd = `${getSpecforgePath()} emit "${spec}"`;
            exec(cmd, { cwd: vscode.workspace.workspaceFolders?.[0]?.uri.fsPath }, (err, stdout, stderr) => {
                if (err) {
                    vscode.window.showErrorMessage(`specforge emit failed: ${stderr}`);
                    return;
                }
                const doc = await vscode.workspace.openTextDocument({
                    content: stdout,
                    language: 'json'
                });
                vscode.window.showTextDocument(doc, { preview: true });
            });
        })
    );
}

export function deactivate() {}
