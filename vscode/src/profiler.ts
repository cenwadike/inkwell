import * as vscode from 'vscode';
import { exec } from 'child_process';
import { promisify } from 'util';
import * as fs from 'fs/promises';
import * as path from 'path';

const execAsync = promisify(exec);

export class InkwellProfiler {
    async profile(filePath: string): Promise<any> {
        const config = vscode.workspace.getConfiguration('inkwell');
        const cliPath = config.get<string>('cliPath', 'inkwell');

        // Get workspace folder
        const workspaceFolder = vscode.workspace.workspaceFolders?.[0];
        if (!workspaceFolder) {
            throw new Error('No workspace folder found');
        }

        const workspacePath = workspaceFolder.uri.fsPath;

        try {
            // Run inkwell CLI
            const { stdout, stderr } = await execAsync(
                `${cliPath} dip "${filePath}" --output json --no-color`,
                {
                    cwd: workspacePath,
                    maxBuffer: 1024 * 1024 * 10, // 10MB buffer
                }
            );

            if (stderr) {
                console.warn('Inkwell stderr:', stderr);
            }

            // Read the decorations file
            const decorationsPath = path.join(workspacePath, '.inkwell', 'decorations.json');
            const decorationsData = await fs.readFile(decorationsPath, 'utf-8');
            const result = JSON.parse(decorationsData);

            return result;
        } catch (error) {
            if (error instanceof Error) {
                // Check if it's a command not found error
                if (error.message.includes('command not found') || error.message.includes('ENOENT')) {
                    throw new Error(
                        'Inkwell CLI not found. Please install it with: cargo install --path ./cli'
                    );
                }
                throw new Error(`Failed to run inkwell: ${error.message}`);
            }
            throw error;
        }
    }
}