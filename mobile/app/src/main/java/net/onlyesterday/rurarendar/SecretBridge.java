package net.onlyesterday.rurarendar;

import android.content.Context;
import android.content.SharedPreferences;
import android.security.keystore.KeyGenParameterSpec;
import android.security.keystore.KeyProperties;
import android.util.Base64;

import java.nio.charset.StandardCharsets;
import java.security.KeyStore;

import javax.crypto.Cipher;
import javax.crypto.KeyGenerator;
import javax.crypto.SecretKey;
import javax.crypto.spec.GCMParameterSpec;

final class SecretBridge {
    private static final String KEYSTORE = "AndroidKeyStore";
    private static final String KEY_ALIAS = "rurarendar-passphrase-wrap";
    private static final String PREFS = "rurarendar-secrets";
    private static final String WRAPPED = "wrapped";
    private static final String IV = "iv";
    private static final String TRANSFORMATION = "AES/GCM/NoPadding";
    private static final int GCM_TAG_BITS = 128;

    private SecretBridge() {}

    public static void store(Context context, String json) throws Exception {
        SecretKey key = wrappingKey();
        Cipher cipher = Cipher.getInstance(TRANSFORMATION);
        cipher.init(Cipher.ENCRYPT_MODE, key);
        byte[] iv = cipher.getIV();
        byte[] wrapped = cipher.doFinal(json.getBytes(StandardCharsets.UTF_8));
        context.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
                .edit()
                .putString(IV, Base64.encodeToString(iv, Base64.NO_WRAP))
                .putString(WRAPPED, Base64.encodeToString(wrapped, Base64.NO_WRAP))
                .apply();
    }

    public static String load(Context context) throws Exception {
        SharedPreferences prefs = context.getSharedPreferences(PREFS, Context.MODE_PRIVATE);
        String ivB64 = prefs.getString(IV, null);
        String wrappedB64 = prefs.getString(WRAPPED, null);
        if (ivB64 == null || wrappedB64 == null) {
            return "";
        }
        SecretKey key = wrappingKey();
        Cipher cipher = Cipher.getInstance(TRANSFORMATION);
        cipher.init(
                Cipher.DECRYPT_MODE,
                key,
                new GCMParameterSpec(GCM_TAG_BITS, Base64.decode(ivB64, Base64.NO_WRAP)));
        byte[] plain = cipher.doFinal(Base64.decode(wrappedB64, Base64.NO_WRAP));
        return new String(plain, StandardCharsets.UTF_8);
    }

    public static void delete(Context context) {
        context.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
                .edit()
                .remove(IV)
                .remove(WRAPPED)
                .apply();
    }

    private static SecretKey wrappingKey() throws Exception {
        KeyStore store = KeyStore.getInstance(KEYSTORE);
        store.load(null);
        if (store.containsAlias(KEY_ALIAS)) {
            return (SecretKey) store.getKey(KEY_ALIAS, null);
        }
        KeyGenerator generator = KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES, KEYSTORE);
        generator.init(
                new KeyGenParameterSpec.Builder(
                                KEY_ALIAS,
                                KeyProperties.PURPOSE_ENCRYPT | KeyProperties.PURPOSE_DECRYPT)
                        .setBlockModes(KeyProperties.BLOCK_MODE_GCM)
                        .setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE)
                        .setKeySize(256)
                        .build());
        return generator.generateKey();
    }
}
