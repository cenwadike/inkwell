import * as vscode from 'vscode';

interface InlineDecoration {
    line: number;
    text: string;
    color: string;
}

interface GutterDecoration {
    line: number;
    icon: string;
    severity: string;
}

interface HoverDecoration {
    line: number;
    markdown: string;
}

interface CodeAction {
    line: number;
    title: string;
    replacement: {
        start_line: number;
        end_line: number;
        new_text: string;
    };
}

interface DecorationData {
    inline: InlineDecoration[];
    gutter: GutterDecoration[];
    hovers: HoverDecoration[];
    code_actions: CodeAction[];
}

export class DecorationManager {
    private decorationTypes: Map<string, vscode.TextEditorDecorationType> = new Map();
    private hoverProvider?: vscode.Disposable;

    constructor() {
        this.setupDecorationTypes();
    }

    private setupDecorationTypes() {
        // Low cost decorations (gray)
        this.decorationTypes.set('low', vscode.window.createTextEditorDecorationType({
            after: {
                margin: '0 0 0 1em',
                fontStyle: 'italic',
            },
            light: {
                after: { color: '#858585' }
            },
            dark: {
                after: { color: '#858585' }
            }
        }));

        // Medium cost decorations (orange)
        this.decorationTypes.set('medium', vscode.window.createTextEditorDecorationType({
            after: {
                margin: '0 0 0 1em',
                fontStyle: 'italic',
            },
            light: {
                after: { color: '#FFA500' }
            },
            dark: {
                after: { color: '#FFA500' }
            }
        }));

        // High cost decorations (red)
        this.decorationTypes.set('high', vscode.window.createTextEditorDecorationType({
            after: {
                margin: '0 0 0 1em',
                fontStyle: 'italic',
                fontWeight: 'bold',
            },
            light: {
                after: { color: '#FF4444' }
            },
            dark: {
                after: { color: '#FF4444' }
            }
        }));

        // Gutter decoration for hotspots (flame icon)
        this.decorationTypes.set('gutter-flame', vscode.window.createTextEditorDecorationType({
            gutterIconPath: this.createFlameIcon(),
            gutterIconSize: 'contain',
        }));

        // Gutter decoration for optimizations (lightbulb icon)
        this.decorationTypes.set('gutter-lightbulb', vscode.window.createTextEditorDecorationType({
            gutterIconPath: this.createLightbulbIcon(),
            gutterIconSize: 'contain',
        }));
    }

    private createFlameIcon(): vscode.Uri {
        // Create a simple SVG flame icon
        const svg = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16">
            <path fill="#FF4444" d="M8 2c-.5 2-1 3-2 4 1 .5 2 1.5 2 3 0 1.5-1 3-3 3 0-1 .5-2 .5-3S4 7 3 7c0 3 2 5 5 5s5-2 5-5c0-2-1-3-2-4-.5 2-1.5 3-3 1z"/>
        </svg>`;
        return vscode.Uri.parse(`data:image/svg+xml;base64,${Buffer.from(svg).toString('base64')}`);
    }

    private createLightbulbIcon(): vscode.Uri {
        // Create a simple SVG lightbulb icon
        const svg = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16">
            <path fill="#FFA500" d="M8 2a4 4 0 0 0-4 4c0 1.5.7 2.8 1.8 3.6V12h4.4V9.6C11.3 8.8 12 7.5 12 6a4 4 0 0 0-4-4zm1.5 9.5h-3V13h3v-1.5z"/>
        </svg>`;
        return vscode.Uri.parse(`data:image/svg+xml;base64,${Buffer.from(svg).toString('base64')}`);
    }

    applyDecorations(editor: vscode.TextEditor, result: any) {
        const config = vscode.workspace.getConfiguration('inkwell');
        const enabled = config.get<boolean>('decorations.enabled', true);
        const showGutter = config.get<boolean>('decorations.showGutterIcons', true);

        if (!enabled) {
            return;
        }

        const decorations = result.decorations as DecorationData;

        // Group inline decorations by color
        const lowDecorations: vscode.DecorationOptions[] = [];
        const mediumDecorations: vscode.DecorationOptions[] = [];
        const highDecorations: vscode.DecorationOptions[] = [];

        for (const dec of decorations.inline) {
            const lineIndex = dec.line - 1; // Convert to 0-based
            if (lineIndex < 0 || lineIndex >= editor.document.lineCount) {
                continue;
            }

            const line = editor.document.lineAt(lineIndex);
            const decoration: vscode.DecorationOptions = {
                range: new vscode.Range(lineIndex, line.text.length, lineIndex, line.text.length),
                renderOptions: {
                    after: {
                        contentText: dec.text,
                    }
                }
            };

            if (dec.color === 'low') {
                lowDecorations.push(decoration);
            } else if (dec.color === 'medium') {
                mediumDecorations.push(decoration);
            } else {
                highDecorations.push(decoration);
            }
        }

        // Apply inline decorations
        editor.setDecorations(this.decorationTypes.get('low')!, lowDecorations);
        editor.setDecorations(this.decorationTypes.get('medium')!, mediumDecorations);
        editor.setDecorations(this.decorationTypes.get('high')!, highDecorations);

        // Apply gutter decorations
        if (showGutter) {
            const flameDecorations: vscode.DecorationOptions[] = [];
            const lightbulbDecorations: vscode.DecorationOptions[] = [];

            for (const dec of decorations.gutter) {
                const lineIndex = dec.line - 1;
                if (lineIndex < 0 || lineIndex >= editor.document.lineCount) {
                    continue;
                }

                const range = editor.document.lineAt(lineIndex).range;
                const decoration: vscode.DecorationOptions = { range };

                if (dec.icon === 'flame') {
                    flameDecorations.push(decoration);
                } else if (dec.icon === 'lightbulb') {
                    lightbulbDecorations.push(decoration);
                }
            }

            editor.setDecorations(this.decorationTypes.get('gutter-flame')!, flameDecorations);
            editor.setDecorations(this.decorationTypes.get('gutter-lightbulb')!, lightbulbDecorations);
        }

        // Register hover provider
        this.registerHoverProvider(decorations.hovers);
    }

    private registerHoverProvider(hovers: HoverDecoration[]) {
        // Dispose old provider
        if (this.hoverProvider) {
            this.hoverProvider.dispose();
        }

        const hoverMap = new Map<number, string>();
        for (const hover of hovers) {
            hoverMap.set(hover.line - 1, hover.markdown);
        }

        this.hoverProvider = vscode.languages.registerHoverProvider('rust', {
            provideHover(document, position) {
                const markdown = hoverMap.get(position.line);
                if (markdown) {
                    return new vscode.Hover(new vscode.MarkdownString(markdown));
                }
                return undefined;
            }
        });
    }

    clearAll() {
        const editor = vscode.window.activeTextEditor;
        if (!editor) {
            return;
        }

        for (const decorationType of this.decorationTypes.values()) {
            editor.setDecorations(decorationType, []);
        }

        if (this.hoverProvider) {
            this.hoverProvider.dispose();
            this.hoverProvider = undefined;
        }
    }
}