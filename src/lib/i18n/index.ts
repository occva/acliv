import en, { type TranslationKey } from './en';
import zh from './zh';

export type Locale = 'zh' | 'en';

type TranslationParams = Record<string, string | number | boolean | null | undefined>;

const dictionaries = {
    zh,
    en,
} satisfies Record<Locale, Record<TranslationKey, string>>;

export const LOCALE_STORAGE_KEY = 'acliv:locale';

let currentLocale: Locale = 'zh';

function normalizeLocale(value?: string | null): Locale | null {
    if (!value) return null;
    const normalized = value.trim().toLowerCase();
    if (normalized.startsWith('zh')) return 'zh';
    if (normalized.startsWith('en')) return 'en';
    return null;
}

function syncDocumentLang(locale: Locale) {
    if (typeof document === 'undefined') return;
    document.documentElement.lang = locale;
}

function detectNavigatorLocale(): Locale {
    if (typeof navigator === 'undefined') return 'zh';
    return navigator.language?.toLowerCase().startsWith('zh') ? 'zh' : 'en';
}

export function getInitialLocale(): Locale {
    if (typeof window === 'undefined') {
        currentLocale = 'zh';
        return currentLocale;
    }

    const stored = normalizeLocale(localStorage.getItem(LOCALE_STORAGE_KEY));
    currentLocale = stored ?? detectNavigatorLocale() ?? 'zh';
    syncDocumentLang(currentLocale);
    return currentLocale;
}

export function getLocale(): Locale {
    if (typeof window === 'undefined') {
        return currentLocale;
    }

    const stored = normalizeLocale(localStorage.getItem(LOCALE_STORAGE_KEY));
    if (stored) {
        currentLocale = stored;
        syncDocumentLang(currentLocale);
        return currentLocale;
    }

    if (!currentLocale) {
        return getInitialLocale();
    }

    syncDocumentLang(currentLocale);
    return currentLocale;
}

export function setLocale(locale: Locale): Locale {
    currentLocale = locale;
    if (typeof window !== 'undefined') {
        localStorage.setItem(LOCALE_STORAGE_KEY, locale);
    }
    syncDocumentLang(locale);
    return locale;
}

export function translate(
    locale: Locale,
    key: TranslationKey,
    params?: TranslationParams,
): string {
    const template = dictionaries[locale][key] ?? dictionaries.en[key] ?? key;
    if (!params) return template;

    return template.replace(/\{\{\s*([\w.]+)\s*\}\}/g, (_, name: string) => {
        const value = params[name];
        return value == null ? '' : String(value);
    });
}

export type { TranslationKey, TranslationParams };
