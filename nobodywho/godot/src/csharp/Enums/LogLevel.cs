namespace NobodyWho.Enums;

/// <summary>
/// The enum that represents the valid log levels for NobodyWho.
/// </summary>
public enum LogLevel
{
    /// <summary>
    /// The <strong>Trace</strong> log level that designates very low priority, often extremely verbose, information.
    /// </summary>
    /// <remarks>
    /// See: <see href="https://docs.rs/tracing/latest/tracing/struct.Level.html#associatedconstant.TRACE"/>
    /// </remarks>
    Trace,

    /// <summary>
    /// The <strong>Debug</strong> log level that designates lower priority information.
    /// </summary>
    /// <remarks>
    /// See: <see href="https://docs.rs/tracing/latest/tracing/struct.Level.html#associatedconstant.DEBUG"/>
    /// </remarks>
    Debug,

    /// <summary>
    /// The <strong>Info</strong> log level that designates useful information.
    /// </summary>
    /// <remarks>
    /// See: <see href="https://docs.rs/tracing/latest/tracing/struct.Level.html#associatedconstant.INFO"/>
    /// </remarks>
    Info,

    /// <summary>
    /// The <strong>Warn</strong> log level that designates hazardous situations.
    /// </summary>
    /// <remarks>
    /// See: <see href="https://docs.rs/tracing/latest/tracing/struct.Level.html#associatedconstant.WARN"/>
    /// </remarks>
    Warn,

    /// <summary>
    /// The <strong>Error</strong> log level that designates very serious errors.
    /// </summary>
    /// <remarks>
    /// See: <see href="https://docs.rs/tracing/latest/tracing/struct.Level.html#associatedconstant.ERROR"/>
    /// </remarks>
    Error
}