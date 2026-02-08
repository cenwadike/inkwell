import * as vscode from 'vscode';
import { InkwellProfiler } from './profiler';
import { DecorationManager } from './decorations';

let profiler: InkwellProfiler;
let decorationManager: DecorationManager;
let statusBarItem: vscode.StatusBarItem;

export function activate(context: vscode.ExtensionContext) {
    console.log('Inkwell extension activated');

    profiler = new InkwellProfiler();
    decorationManager = new DecorationManager();

    // Create status bar item
    statusBarItem = vscode.window.createStatusBarItem(vscode.StatusBarAlignment.Right, 100);
    statusBarItem.command = 'inkwell.profileCurrentFile';
    context.subscriptions.push(statusBarItem);

    // Register commands
    context.subscriptions.push(
        vscode.commands.registerCommand('inkwell.profileCurrentFile', async () => {
            await profileCurrentFile();
        })
    );

    context.subscriptions.push(
        vscode.commands.registerCommand('inkwell.clearDecorations', () => {
            decorationManager.clearAll();
            statusBarItem.hide();
        })
    );

    context.subscriptions.push(
        vscode.commands.registerCommand('inkwell.toggleAutoProfile', () => {
            const config = vscode.workspace.getConfiguration('inkwell');
            const current = config.get<boolean>('autoProfile', false);
            config.update('autoProfile', !current, vscode.ConfigurationTarget.Global);
            vscode.window.showInformationMessage(
                `Auto-profile ${!current ? 'enabled' : 'disabled'}`
            );
        })
    );

    // Watch for file saves if auto-profile is enabled
    context.subscriptions.push(
        vscode.workspace.onDidSaveTextDocument(async (document) => {
            const config = vscode.workspace.getConfiguration('inkwell');
            const autoProfile = config.get<boolean>('autoProfile', false);

            if (autoProfile && document.languageId === 'rust') {
                await profileDocument(document);
            }
        })
    );

    // Watch for active editor changes
    context.subscriptions.push(
        vscode.window.onDidChangeActiveTextEditor((editor) => {
            if (editor) {
                updateDecorationsForEditor(editor);
            }
        })
    );
}

async function profileCurrentFile() {
    const editor = vscode.window.activeTextEditor;
    if (!editor) {
        vscode.window.showWarningMessage('No active file to profile');
        return;
    }

    if (editor.document.languageId !== 'rust') {
        vscode.window.showWarningMessage('Inkwell only works with Rust files');
        return;
    }

    await profileDocument(editor.document);
}

async function profileDocument(document: vscode.TextDocument) {
    try {
        // Show progress
        await vscode.window.withProgress(
            {
                location: vscode.ProgressLocation.Notification,
                title: 'Inkwell: Profiling contract...',
                cancellable: false,
            },
            async (progress) => {
                progress.report({ increment: 0 });

                // Run profiler
                const result = await profiler.profile(document.uri.fsPath);

                progress.report({ increment: 50 });

                if (result) {
                    // Apply decorations
                    const editor = vscode.window.activeTextEditor;
                    if (editor && editor.document === document) {
                        decorationManager.applyDecorations(editor, result);
                    }

                    // Update status bar
                    updateStatusBar(result);

                    vscode.window.showInformationMessage(
                        `Profiled ${result.function}: ${formatNumber(result.total_ink)} ink (≈ ${result.gas_equivalent} gas)`
                    );
                }

                progress.report({ increment: 100 });
            }
        );
    } catch (error) {
        const message = error instanceof Error ? error.message : String(error);
        vscode.window.showErrorMessage(`Inkwell error: ${message}`);
    }
}

function updateDecorationsForEditor(editor: vscode.TextEditor) {
    // Check if we have decorations for this file
    const workspaceFolder = vscode.workspace.getWorkspaceFolder(editor.document.uri);
    if (!workspaceFolder) {
        return;
    }

    const decorationsPath = vscode.Uri.joinPath(workspaceFolder.uri, '.inkwell', 'decorations.json');

    vscode.workspace.fs.readFile(decorationsPath).then(
        (data) => {
            try {
                const result = JSON.parse(data.toString());

                // Only apply if the file paths match
                // The 'result' object needs to contain the path of the profiled file
                if (result && result.filePath === editor.document.uri.fsPath) {
                    decorationManager.applyDecorations(editor, result);
                    updateStatusBar(result);
                }

            } catch (error) {
                console.error('Failed to parse decorations:', error);
            }
        },
        () => {
            // File doesn't exist, ignore
            // You might want to clear decorations here for non-profiled files
            decorationManager.clearAll();
            statusBarItem.hide();
        }
    );
}

function updateStatusBar(result: any) {
    const ink = formatNumber(result.total_ink);
    const gas = result.gas_equivalent;
    const hotspots = result.decorations?.gutter?.filter((g: any) => g.icon === 'flame').length || 0;

    statusBarItem.text = `💰 ${ink} ink (≈ ${gas} gas) | 🔥 ${hotspots} hotspots`;
    statusBarItem.show();
}

function formatNumber(num: number): string {
    if (num >= 1_000_000) {
        return `${(num / 1_000_000).toFixed(1)}M`;
    } else if (num >= 1_000) {
        return `${(num / 1_000).toFixed(0)}K`;
    }
    return num.toString();
}

export function deactivate() {
    decorationManager.clearAll();
}