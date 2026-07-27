//! Internationalization (i18n) support for generated SDK error messages.
//!
//! The [`I18nConfig`] holds a map of locale codes to translation key-value
//! pairs and can generate language-specific runtime files for each emitter.

use indexmap::IndexMap;

/// Configuration for SDK internationalization.
///
/// Stores translations keyed by locale code (e.g. `"en"`, `"es"`). Each locale
/// maps dot-separated keys (e.g. `"errors.notFound"`) to localized strings.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct I18nConfig {
    /// The default locale used when no explicit locale is requested.
    pub default_locale: String,
    /// Locale code -> (translation key -> translated string).
    pub translations: IndexMap<String, IndexMap<String, String>>,
}

impl I18nConfig {
    /// Create an empty i18n config with English as the default locale.
    pub fn new() -> Self {
        Self {
            default_locale: "en".into(),
            translations: IndexMap::new(),
        }
    }

    /// Build an `I18nConfig` from a comma-separated list of locale codes,
    /// populated with the built-in default error translations.
    pub fn from_locales(locales: &[String]) -> Self {
        let mut config = Self::new();
        for locale in locales {
            let messages = default_translations(locale);
            config.translations.insert(locale.clone(), messages);
        }
        config
    }

    /// Look up a translated string for the given key and locale.
    /// Returns the key itself if no translation is found (safe fallback).
    pub fn t<'a>(&'a self, key: &'a str, locale: &str) -> &'a str {
        self.translations
            .get(locale)
            .and_then(|m| m.get(key).map(|s| s.as_str()))
            .unwrap_or(key)
    }

    /// Return the list of configured locale codes.
    pub fn locales(&self) -> Vec<&str> {
        self.translations.keys().map(|s| s.as_str()).collect()
    }
}

impl Default for I18nConfig {
    fn default() -> Self {
        Self::new()
    }
}

/// Return the built-in default error translations for a locale.
///
/// These are the standard SDK error messages that every generated SDK uses:
/// HTTP errors, network errors, timeout errors, configuration errors, and
/// validation errors.
fn default_translations(locale: &str) -> IndexMap<String, String> {
    let mut m = IndexMap::new();
    match locale {
        "en" => {
            m.insert("errors.http".into(), "HTTP {status}: {body}".into());
            m.insert("errors.network".into(), "Network error".into());
            m.insert("errors.timeout".into(), "Request timed out after {elapsed}ms".into());
            m.insert("errors.configuration".into(), "Configuration error".into());
            m.insert("errors.validation".into(), "Validation failed".into());
            m.insert("errors.notFound".into(), "Resource not found".into());
            m.insert("errors.unauthorized".into(), "Unauthorized".into());
            m.insert("errors.forbidden".into(), "Forbidden".into());
            m.insert("errors.rateLimited".into(), "Rate limit exceeded".into());
            m.insert("errors.serverError".into(), "Internal server error".into());
            m.insert("errors.retriesExhausted".into(), "Exhausted retries without a response".into());
            m.insert("errors.requestAborted".into(), "Request aborted by caller".into());
        }
        "es" => {
            m.insert("errors.http".into(), "HTTP {status}: {body}".into());
            m.insert("errors.network".into(), "Error de red".into());
            m.insert("errors.timeout".into(), "La solicitud expiró después de {elapsed}ms".into());
            m.insert("errors.configuration".into(), "Error de configuración".into());
            m.insert("errors.validation".into(), "Validación fallida".into());
            m.insert("errors.notFound".into(), "Recurso no encontrado".into());
            m.insert("errors.unauthorized".into(), "No autorizado".into());
            m.insert("errors.forbidden".into(), "Prohibido".into());
            m.insert("errors.rateLimited".into(), "Límite de velocidad excedido".into());
            m.insert("errors.serverError".into(), "Error interno del servidor".into());
            m.insert("errors.retriesExhausted".into(), "Reintentos agotados sin respuesta".into());
            m.insert("errors.requestAborted".into(), "Solicitud cancelada por el usuario".into());
        }
        "fr" => {
            m.insert("errors.http".into(), "HTTP {status} : {body}".into());
            m.insert("errors.network".into(), "Erreur réseau".into());
            m.insert("errors.timeout".into(), "La requête a expiré après {elapsed}ms".into());
            m.insert("errors.configuration".into(), "Erreur de configuration".into());
            m.insert("errors.validation".into(), "Échec de la validation".into());
            m.insert("errors.notFound".into(), "Ressource non trouvée".into());
            m.insert("errors.unauthorized".into(), "Non autorisé".into());
            m.insert("errors.forbidden".into(), "Interdit".into());
            m.insert("errors.rateLimited".into(), "Limite de débit dépassée".into());
            m.insert("errors.serverError".into(), "Erreur interne du serveur".into());
            m.insert("errors.retriesExhausted".into(), "Tentatives épuisées sans réponse".into());
            m.insert("errors.requestAborted".into(), "Requête annulée par l'utilisateur".into());
        }
        "de" => {
            m.insert("errors.http".into(), "HTTP {status}: {body}".into());
            m.insert("errors.network".into(), "Netzwerkfehler".into());
            m.insert("errors.timeout".into(), "Zeitüberschreitung nach {elapsed}ms".into());
            m.insert("errors.configuration".into(), "Konfigurationsfehler".into());
            m.insert("errors.validation".into(), "Validierung fehlgeschlagen".into());
            m.insert("errors.notFound".into(), "Ressource nicht gefunden".into());
            m.insert("errors.unauthorized".into(), "Nicht autorisiert".into());
            m.insert("errors.forbidden".into(), "Verboten".into());
            m.insert("errors.rateLimited".into(), "Ratenlimit überschritten".into());
            m.insert("errors.serverError".into(), "Interner Serverfehler".into());
            m.insert("errors.retriesExhausted".into(), "Wiederholungen ohne Antwort erschöpft".into());
            m.insert("errors.requestAborted".into(), "Anfrage vom Aufrufer abgebrochen".into());
        }
        "ja" => {
            m.insert("errors.http".into(), "HTTP {status}: {body}".into());
            m.insert("errors.network".into(), "ネットワークエラー".into());
            m.insert("errors.timeout".into(), "リクエストが{elapsed}ms後にタイムアウトしました".into());
            m.insert("errors.configuration".into(), "設定エラー".into());
            m.insert("errors.validation".into(), "検証に失敗しました".into());
            m.insert("errors.notFound".into(), "リソースが見つかりません".into());
            m.insert("errors.unauthorized".into(), "未認証".into());
            m.insert("errors.forbidden".into(), "禁止".into());
            m.insert("errors.rateLimited".into(), "レート制限を超過しました".into());
            m.insert("errors.serverError".into(), "内部サーバーエラー".into());
            m.insert("errors.retriesExhausted".into(), "レスポンスなしでリトライが尽きました".into());
            m.insert("errors.requestAborted".into(), "呼び出し元によりリクエストが中止されました".into());
        }
        "zh" => {
            m.insert("errors.http".into(), "HTTP {status}: {body}".into());
            m.insert("errors.network".into(), "网络错误".into());
            m.insert("errors.timeout".into(), "请求在{elapsed}ms后超时".into());
            m.insert("errors.configuration".into(), "配置错误".into());
            m.insert("errors.validation".into(), "验证失败".into());
            m.insert("errors.notFound".into(), "资源未找到".into());
            m.insert("errors.unauthorized".into(), "未授权".into());
            m.insert("errors.forbidden".into(), "禁止访问".into());
            m.insert("errors.rateLimited".into(), "超过速率限制".into());
            m.insert("errors.serverError".into(), "内部服务器错误".into());
            m.insert("errors.retriesExhausted".into(), "重试次数已用尽，无响应".into());
            m.insert("errors.requestAborted".into(), "请求已被调用方中止".into());
        }
        "pt" => {
            m.insert("errors.http".into(), "HTTP {status}: {body}".into());
            m.insert("errors.network".into(), "Erro de rede".into());
            m.insert("errors.timeout".into(), "A requisição expirou após {elapsed}ms".into());
            m.insert("errors.configuration".into(), "Erro de configuração".into());
            m.insert("errors.validation".into(), "Falha na validação".into());
            m.insert("errors.notFound".into(), "Recurso não encontrado".into());
            m.insert("errors.unauthorized".into(), "Não autorizado".into());
            m.insert("errors.forbidden".into(), "Proibido".into());
            m.insert("errors.rateLimited".into(), "Limite de taxa excedido".into());
            m.insert("errors.serverError".into(), "Erro interno do servidor".into());
            m.insert("errors.retriesExhausted".into(), "Tentativas esgotadas sem resposta".into());
            m.insert("errors.requestAborted".into(), "Requisição cancelada pelo chamador".into());
        }
        "ko" => {
            m.insert("errors.http".into(), "HTTP {status}: {body}".into());
            m.insert("errors.network".into(), "네트워크 오류".into());
            m.insert("errors.timeout".into(), "요청이 {elapsed}ms 후에 시간 초과됨".into());
            m.insert("errors.configuration".into(), "구성 오류".into());
            m.insert("errors.validation".into(), "검증 실패".into());
            m.insert("errors.notFound".into(), "리소스를 찾을 수 없음".into());
            m.insert("errors.unauthorized".into(), "인증되지 않음".into());
            m.insert("errors.forbidden".into(), "접근 금지".into());
            m.insert("errors.rateLimited".into(), "속도 제한 초과".into());
            m.insert("errors.serverError".into(), "내부 서버 오류".into());
            m.insert("errors.retriesExhausted".into(), "응답 없이 재시도 횟수 소진".into());
            m.insert("errors.requestAborted".into(), "호출자에 의해 요청이 중단됨".into());
        }
        _ => {
            // Unknown locale: use the key itself as the translation (safe fallback).
            for key in [
                "errors.http", "errors.network", "errors.timeout",
                "errors.configuration", "errors.validation", "errors.notFound",
                "errors.unauthorized", "errors.forbidden", "errors.rateLimited",
                "errors.serverError", "errors.retriesExhausted", "errors.requestAborted",
            ] {
                m.insert(key.into(), key.into());
            }
        }
    }
    m
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = I18nConfig::new();
        assert_eq!(config.default_locale, "en");
        assert!(config.translations.is_empty());
    }

    #[test]
    fn test_from_locales() {
        let config = I18nConfig::from_locales(&["en".into(), "es".into()]);
        assert_eq!(config.locales(), vec!["en", "es"]);
        assert_eq!(config.t("errors.notFound", "en"), "Resource not found");
        assert_eq!(config.t("errors.notFound", "es"), "Recurso no encontrado");
    }

    #[test]
    fn test_fallback_to_key() {
        let config = I18nConfig::from_locales(&["en".into()]);
        assert_eq!(config.t("errors.unknown", "en"), "errors.unknown");
        assert_eq!(config.t("errors.notFound", "fr"), "errors.notFound");
    }

    #[test]
    fn test_unknown_locale_gets_key_fallback() {
        let config = I18nConfig::from_locales(&["zz".into()]);
        assert_eq!(config.t("errors.notFound", "zz"), "errors.notFound");
    }
}
