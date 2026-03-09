package com.nobodywho

import android.content.ContentValues
import android.content.Context
import android.database.sqlite.SQLiteDatabase
import android.database.sqlite.SQLiteOpenHelper
import org.json.JSONObject
import java.nio.ByteBuffer
import java.nio.ByteOrder
import java.util.concurrent.locks.ReentrantLock
import kotlin.concurrent.withLock
import kotlin.math.sqrt

// MARK: - InMemoryVectorStore

/** An in-memory vector store. Fast but non-persistent — data is lost when the process exits. */
class InMemoryVectorStore : VectorStore {
    private data class Entry(
        val id: String,
        val vector: FloatArray,
        val text: String,
        val metadata: Map<String, String>
    )

    private val documents = mutableListOf<Entry>()

    override fun add(id: String, vector: FloatArray, metadata: Map<String, String>) {
        val text = metadata["text"] ?: ""
        documents.removeAll { it.id == id }
        documents.add(Entry(id, vector, text, metadata))
    }

    override fun search(query: FloatArray, topK: Int): List<ScoredDocument> =
        documents
            .map { doc ->
                ScoredDocument(
                    id = doc.id,
                    text = doc.text,
                    score = cosineSimilarity(query, doc.vector),
                    metadata = doc.metadata
                )
            }
            .sortedByDescending { it.score }
            .take(topK)

    override fun remove(id: String) {
        documents.removeAll { it.id == id }
    }

    override fun clear() {
        documents.clear()
    }

    override val count: Int get() = documents.size
}

// MARK: - SQLiteVectorStore

/** A persistent vector store backed by Android's SQLite. */
class SQLiteVectorStore(context: Context, databaseName: String = "nobodywho_vectors.db") : VectorStore {
    private val db: SQLiteDatabase
    private val lock = ReentrantLock()

    init {
        val helper = object : SQLiteOpenHelper(context, databaseName, null, 1) {
            override fun onCreate(db: SQLiteDatabase) {
                db.execSQL("""
                    CREATE TABLE IF NOT EXISTS vectors (
                        id TEXT PRIMARY KEY,
                        vector BLOB NOT NULL,
                        text TEXT,
                        metadata TEXT,
                        created_at INTEGER DEFAULT (strftime('%s', 'now'))
                    )
                """.trimIndent())
                db.execSQL("CREATE INDEX IF NOT EXISTS idx_created_at ON vectors(created_at)")
            }

            override fun onUpgrade(db: SQLiteDatabase, oldVersion: Int, newVersion: Int) {}
        }
        db = helper.writableDatabase
    }

    override fun add(id: String, vector: FloatArray, metadata: Map<String, String>) {
        val text = metadata["text"] ?: ""
        val metadataJson = JSONObject(metadata).toString()
        val vectorBytes = floatsToBytes(vector)

        val values = ContentValues().apply {
            put("id", id)
            put("vector", vectorBytes)
            put("text", text)
            put("metadata", metadataJson)
        }

        lock.withLock {
            db.insertWithOnConflict("vectors", null, values, SQLiteDatabase.CONFLICT_REPLACE)
        }
    }

    override fun search(query: FloatArray, topK: Int): List<ScoredDocument> {
        val results = mutableListOf<ScoredDocument>()

        lock.withLock {
            db.rawQuery("SELECT id, vector, text, metadata FROM vectors", null).use { cursor ->
                while (cursor.moveToNext()) {
                    val id = cursor.getString(0)
                    val vectorBytes = cursor.getBlob(1)
                    val text = cursor.getString(2) ?: ""
                    val metadataJson = cursor.getString(3) ?: "{}"

                    val vector = bytesToFloats(vectorBytes)
                    val metadata = jsonToMap(metadataJson)

                    results.add(
                        ScoredDocument(
                            id = id,
                            text = text,
                            score = cosineSimilarity(query, vector),
                            metadata = metadata
                        )
                    )
                }
            }
        }

        return results.sortedByDescending { it.score }.take(topK)
    }

    override fun remove(id: String) {
        lock.withLock {
            db.delete("vectors", "id = ?", arrayOf(id))
        }
    }

    override fun clear() {
        lock.withLock {
            db.delete("vectors", null, null)
        }
    }

    override val count: Int
        get() = lock.withLock {
            db.rawQuery("SELECT COUNT(*) FROM vectors", null).use { cursor ->
                if (cursor.moveToFirst()) cursor.getInt(0) else 0
            }
        }

    // MARK: - Helpers

    private fun floatsToBytes(floats: FloatArray): ByteArray {
        val buffer = ByteBuffer.allocate(floats.size * 4).order(ByteOrder.LITTLE_ENDIAN)
        floats.forEach { buffer.putFloat(it) }
        return buffer.array()
    }

    private fun bytesToFloats(bytes: ByteArray): FloatArray {
        val buffer = ByteBuffer.wrap(bytes).order(ByteOrder.LITTLE_ENDIAN)
        return FloatArray(bytes.size / 4) { buffer.getFloat() }
    }

    private fun jsonToMap(json: String): Map<String, String> {
        return try {
            val obj = JSONObject(json)
            obj.keys().asSequence().associateWith { obj.optString(it) }
        } catch (e: Exception) {
            emptyMap()
        }
    }
}

// MARK: - Helpers

internal fun cosineSimilarity(a: FloatArray, b: FloatArray): Float {
    if (a.size != b.size) return 0f
    var dot = 0f
    var magA = 0f
    var magB = 0f
    for (i in a.indices) {
        dot += a[i] * b[i]
        magA += a[i] * a[i]
        magB += b[i] * b[i]
    }
    val denom = sqrt(magA) * sqrt(magB)
    return if (denom > 0f) dot / denom else 0f
}
