var identityList = [];
var selectedIdentityHash = null;
var activeIdentityHash = null;

function shareAddress(address, displayName) {
    if (!navigator.share) {
        if (navigator.clipboard) {
            navigator.clipboard.writeText(address).then(function() {
                showCopyConfirmationToast('Address');
            });
        }
        return;
    }
    var title = displayName ? displayName + ' (Ratspeak)' : 'Ratspeak Address';
    navigator.share({
        title: title,
        text: address
    }).catch(function() {});
}

function renderNetworkIdentityCard() {
    var container = document.getElementById('net-identity-card');
    if (!container) return;

    var active = null;
    for (var i = 0; i < identityList.length; i++) {
        if (identityList[i].is_active) { active = identityList[i]; break; }
    }

    if (!active) {
        container.innerHTML = '<div class="text-muted-color text-sm">No active identity.</div>';
        return;
    }

    var nickname = escapeHtml(active.display_name || active.nickname || 'Unnamed');
    var lxmfHash = active.lxmf_hash || '';
    var avatarHtml = identityAvatar(lxmfHash, 40);

    container.innerHTML =
        '<div class="identity-summary-inline">' +
            '<div class="identity-summary-avatar">' + avatarHtml + '</div>' +
            '<div class="identity-summary-meta">' +
                '<div class="identity-summary-name">' + nickname + '</div>' +
                '<div class="font-mono inline-hint-sm identity-summary-hash">' + lxmfHash + '</div>' +
            '</div>' +
        '</div>';
}

// Blockies-style SVG identicon. Adapted from https://github.com/download13/blockies (MIT).
var blockies = (function() {
    function seedRand(seed) {
        var s = [0, 0, 0, 0];
        for (var i = 0; i < seed.length; i++) {
            s[i % 4] = (s[i % 4] << 5) - s[i % 4] + seed.charCodeAt(i);
        }
        return function() {
            var t = s[0] ^ (s[0] << 11);
            s[0] = s[1]; s[1] = s[2]; s[2] = s[3];
            s[3] = (s[3] ^ (s[3] >> 19) ^ t ^ (t >> 8)) >>> 0;
            return s[3] / ((1 << 31) >>> 0);
        };
    }

    function createColor(rand) {
        var h = Math.floor(rand() * 360);
        var s = ((rand() * 60) + 40);
        var l = ((rand() + rand() + rand() + rand()) * 25);
        return 'hsl(' + h + ',' + s + '%,' + l + '%)';
    }

    function createImageData(rand, gridSize) {
        var w = gridSize, h = gridSize;
        var halfW = Math.ceil(w / 2);
        var data = [];
        for (var y = 0; y < h; y++) {
            var row = [];
            for (var x = 0; x < halfW; x++) {
                // 0 = bg, 1 = primary, 2 = spot
                row.push(Math.floor(rand() * 2.3));
            }
            var fullRow = row.slice();
            for (var x = Math.floor(w / 2) - 1; x >= 0; x--) {
                fullRow.push(row[x]);
            }
            data.push(fullRow);
        }
        return data;
    }

    var fn = function(seed, svgSize) {
        var gridSize = 8;
        var rand = seedRand(seed || '');
        var color = createColor(rand);
        var bgcolor = createColor(rand);
        var spotcolor = createColor(rand);
        var grid = createImageData(rand, gridSize);

        var rects = '';
        for (var y = 0; y < gridSize; y++) {
            for (var x = 0; x < gridSize; x++) {
                var val = grid[y][x];
                var fill = val === 0 ? bgcolor : val === 1 ? color : spotcolor;
                rects += '<rect x="' + x + '" y="' + y + '" width="1" height="1" fill="' + fill + '"/>';
            }
        }

        var r = gridSize / 2;
        var clipId = 'bc' + (++blockies._uid);
        return '<svg xmlns="http://www.w3.org/2000/svg" width="' + svgSize +
            '" height="' + svgSize + '" viewBox="0 0 ' + gridSize + ' ' + gridSize +
            '" shape-rendering="crispEdges">' +
            '<clipPath id="' + clipId + '"><circle cx="' + r + '" cy="' + r + '" r="' + r + '"/></clipPath>' +
            '<g clip-path="url(#' + clipId + ')">' + rects + '</g></svg>';
    };
    fn._uid = 0;
    return fn;
})();

// Cache avatars per (hash, size) — blockies PRNG + 64 SVG rects is expensive per call.
var _avatarCache = {};
function identityAvatar(hashValue, size) {
    if (!hashValue) {
        var color = 'var(--text-muted)';
        return '<svg width="' + size + '" height="' + size + '" viewBox="0 0 ' + size + ' ' + size + '">' +
            '<rect width="' + size + '" height="' + size + '" rx="6" fill="' + color + '" opacity="0.3"/>' +
            '</svg>';
    }
    var key = hashValue + '|' + size;
    if (_avatarCache[key]) return _avatarCache[key];
    var svg = blockies(hashValue, size);
    _avatarCache[key] = svg;
    return svg;
}

function loadIdentities(retryCount) {
    retryCount = retryCount || 0;
    RS.invoke('api_list_identities').then(function(data) {
        identityList = data || [];
        var _activeIdent = null;
        for (var i = 0; i < identityList.length; i++) {
            if (identityList[i].is_active) {
                activeIdentityHash = identityList[i].hash;
                _activeIdent = identityList[i];
                break;
            }
        }
        // DB-backed update — survives a race with LXMF init on startup.
        if (_activeIdent && typeof updateHeaderIdentity === 'function') {
            updateHeaderIdentity(
                _activeIdent.lxmf_hash || _activeIdent.hash || '',
                _activeIdent.display_name || _activeIdent.nickname || ''
            );
        }
        document.body.classList.toggle('multi-identity', identityList.length > 1);
        // Per-section try/catch — one render failure shouldn't block others.
        try { renderActiveIdentityCard(); }
        catch(e) { window.RS.diag('error', '[Identity] Active card render error:', e); }

        try { renderIdentityList(); }
        catch(e) { window.RS.diag('error', '[Identity] List render error:', e); }

        try { renderNetworkIdentityCard(); }
        catch(e) {}

        try { if (typeof renderMsgProfileStrip === 'function') renderMsgProfileStrip(); }
        catch(e) {}
    }).catch(function(err) {
        window.RS.diag('error', '[Identity] Failed to load identities:', err);
        if (retryCount < 3) {
            setTimeout(function() { loadIdentities(retryCount + 1); }, 1000 * (retryCount + 1));
        }
    });
}

function renderActiveIdentityCard() {
    var container = document.getElementById('identity-active-card');
    if (!container) return;

    var active = null;
    for (var i = 0; i < identityList.length; i++) {
        if (identityList[i].is_active) {
            active = identityList[i];
            break;
        }
    }

    if (!active) {
        container.innerHTML = '<div class="text-muted-color text-sm">No active identity.</div>';
        return;
    }

    var nickname = escapeHtml(active.display_name || active.nickname || 'Unnamed');
    var displayName = active.display_name || '';
    var lxmfHash = active.lxmf_hash || '';
    var identityHash = active.hash || '';

    var avatarHtml = identityAvatar(lxmfHash || identityHash, 64);

    container.innerHTML =
        '<div class="identity-active-row">' +
            '<div class="identity-avatar">' + avatarHtml + '</div>' +
            '<div class="identity-active-info">' +
                '<div class="identity-card-nickname">' + nickname + '</div>' +
                '<div class="identity-field identity-copyable-address" id="identity-share-address">' +
                    '<span class="identity-label">LXMF Address <svg width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" style="vertical-align:-1px;opacity:0.5"><circle cx="18" cy="5" r="3"/><circle cx="6" cy="12" r="3"/><circle cx="18" cy="19" r="3"/><line x1="8.59" y1="13.51" x2="15.42" y2="17.49"/><line x1="15.41" y1="6.51" x2="8.59" y2="10.49"/></svg></span>' +
                    '<span class="identity-value mono">' + copyableHash(lxmfHash) + '</span>' +
                '</div>' +
                '<div class="identity-field">' +
                    '<span class="identity-label">Destination Hash</span>' +
                    '<span class="identity-value mono">' + copyableHash(identityHash) + '</span>' +
                '</div>' +
            '</div>' +
        '</div>' +
        '<div class="identity-active-controls">' +
            '<div class="modal-field">' +
                '<label>Display Name</label>' +
                '<div class="settings-display-name-row">' +
                    '<input type="text" id="identity-display-name" class="modal-input" placeholder="Optional" maxlength="32" value="' + escapeHtml(displayName) + '">' +
                    '<button class="nr-btn" id="identity-save-name-btn" style="display:none;">Save</button>' +
                '</div>' +
            '</div>' +
        '</div>';

    var shareAddr = document.getElementById('identity-share-address');
    if (shareAddr) {
        shareAddr.addEventListener('click', function(e) {
            e.stopPropagation();
            shareAddress(lxmfHash, active.display_name || active.nickname || '');
        });
    }

    var nameInput = document.getElementById('identity-display-name');
    var saveBtn = document.getElementById('identity-save-name-btn');
    if (nameInput && saveBtn) {
        nameInput.addEventListener('input', function() {
            var current = nameInput.value.trim();
            saveBtn.style.display = (current !== displayName) ? '' : 'none';
        });
        nameInput.addEventListener('keydown', function(e) {
            if (e.key === 'Enter') {
                e.preventDefault();
                if (saveBtn.style.display !== 'none' && !saveBtn.disabled) saveBtn.click();
            }
        });
        saveBtn.addEventListener('click', function() {
            var newName = nameInput.value.trim();
            saveBtn.disabled = true;
            saveBtn.textContent = 'Saving...';
            RS.invoke('api_set_display_name', { args: { display_name: newName } }).then(function() {
                showToast('Display name saved and announced', 'toast-green', 3000);
                saveBtn.textContent = 'Saved!';
                saveBtn.className = 'nr-btn nr-btn-success';
                setTimeout(function() {
                    saveBtn.style.display = 'none';
                    saveBtn.textContent = 'Save';
                    saveBtn.className = 'nr-btn';
                    saveBtn.disabled = false;
                }, 1500);
                loadIdentities();
            }).catch(function(err) {
                saveBtn.textContent = 'Save';
                saveBtn.disabled = false;
                showToast((err && err.message) ? err.message : 'Failed to save', 'toast-red', 3000);
            });
        });
    }

}

function renderIdentityList() {
    var container = document.getElementById('identity-list');
    if (!container) return;

    if (identityList.length === 0) {
        container.innerHTML = '<div class="inline-hint" style="padding:12px;">No identities found.</div>';
        return;
    }

    // Sort by creation time — original identity stays at position 0.
    var sorted = identityList.slice().sort(function(a, b) {
        return (a.created_at || 0) - (b.created_at || 0);
    });

    container.innerHTML = '';
    sorted.forEach(function(ident, index) {
        var item = document.createElement('div');
        item.className = 'identity-list-item';
        if (ident.hash === selectedIdentityHash) item.classList.add('selected');
        if (ident.is_active) item.classList.add('active-identity');

        var nickname = escapeHtml(ident.display_name || ident.nickname || 'Unnamed');
        var lxmfHash = ident.lxmf_hash || '';
        var isOriginal = (index === 0);

        var badgeHtml = '';
        if (isOriginal) {
            badgeHtml += '<svg class="identity-lock-icon" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><rect x="3" y="11" width="18" height="11" rx="2" ry="2"/><path d="M7 11V7a5 5 0 0 1 10 0v4"/></svg>';
        }
        if (ident.is_active) {
            badgeHtml += '<span class="identity-active-badge">Active</span>';
        } else {
            badgeHtml += '<span class="identity-select-btn" data-hash="' + escapeHtml(ident.hash) + '">Select</span>';
        }

        item.innerHTML =
            '<div class="identity-list-avatar">' + identityAvatar(ident.lxmf_hash || ident.hash || '', 32) + '</div>' +
            '<div class="identity-list-info">' +
                '<span class="identity-list-name">' + nickname + '</span>' +
                '<span class="identity-list-hash mono">' + escapeHtml(lxmfHash) + '</span>' +
            '</div>' +
            '<div class="identity-list-badges">' + badgeHtml + '</div>';

        item.addEventListener('click', function(e) {
            if (e.target.classList.contains('identity-select-btn')) return;
            // Original and active identities are not removable, so block selection.
            if (isOriginal || ident.is_active) return;
            selectedIdentityHash = ident.hash;
            renderIdentityList();
        });

        container.appendChild(item);
    });

    if (!container._selectDelegated) {
        container._selectDelegated = true;
        container.addEventListener('click', function(e) {
            if (e.target.classList.contains('identity-select-btn')) {
                var hash = e.target.getAttribute('data-hash');
                if (hash) switchToIdentity(hash);
            }
        });
    }

    updateRemoveButtonState();
}

function updateRemoveButtonState() {
    var removeBtn = document.getElementById('identity-remove-btn');
    if (!removeBtn) return;

    var sorted = identityList.slice().sort(function(a, b) {
        return (a.created_at || 0) - (b.created_at || 0);
    });
    var originalHash = sorted.length > 0 ? sorted[0].hash : null;

    var isOriginal = selectedIdentityHash && selectedIdentityHash === originalHash;
    var isActive = selectedIdentityHash && selectedIdentityHash === activeIdentityHash;

    if (!selectedIdentityHash || isOriginal || isActive) {
        removeBtn.disabled = true;
        if (isOriginal) removeBtn.title = 'The original identity cannot be removed';
        else if (isActive) removeBtn.title = 'The active identity cannot be removed';
        else removeBtn.title = 'Select an identity to remove';
    } else {
        removeBtn.disabled = false;
        removeBtn.title = 'Remove selected identity';
    }
}

function switchToIdentity(hash) {
    var card = document.getElementById('identity-active-card');
    if (card) {
        card.innerHTML = '<div class="identity-switching-overlay"><div class="identity-switching-text">Switching identity...</div></div>';
    }
    // Backend reloads LXMF manager + emits identity_switched, which drives
    // full state cleanup + re-emits initial state for the new identity.
    RS.invoke('switch_identity', { hash: hash }).catch(function() {});
}

function openIdentitySwitcher() {
    if (!identityList || identityList.length <= 1) return;
    var choices = identityList.slice().sort(function(a, b) {
        return (a.created_at || 0) - (b.created_at || 0);
    }).map(function(ident) {
        var name = ident.display_name || ident.nickname || 'Unnamed';
        var hash = ident.lxmf_hash || '';
        var shortLabel = hash ? (typeof shortHash === 'function' ? shortHash(hash, 8, 4) : hash.substring(0, 12) + '\u2026') : '';
        return {
            label: name + (shortLabel ? '  ' + shortLabel : ''),
            value: ident.hash,
            hint: ident.is_active ? 'Currently active' : null
        };
    });
    rsChoice({ title: 'Switch Identity', choices: choices }).then(function(hash) {
        if (!hash || hash === activeIdentityHash) return;
        switchToIdentity(hash);
    });
}

function createNewIdentity() {
    showIdentityModal('Create New Identity',
        '<div class="modal-field">' +
            '<label>Display Name</label>' +
            '<input type="text" id="identity-modal-nickname" class="modal-input" placeholder="e.g. Rat King" maxlength="32">' +
        '</div>',
        function() {
            var nickname = document.getElementById('identity-modal-nickname').value.trim();
            return RS.invoke('api_create_identity', { args: { nickname: nickname } }).then(function() {
                showToast('Identity created', 'toast-green', 3000);
                closeIdentityModal();
            }).catch(function(err) {
                showToast(err && err.message ? err.message : 'Failed to create identity', 'toast-red', 3000);
            }).then(function() {
                // Refresh even on error — core may have created the row before timing out.
                loadIdentities();
            });
        }
    );
}

function importIdentity() {
    var fileInput = document.getElementById('identity-file-input');
    if (fileInput) fileInput.click();
}

function handleImportFile(file) {
    if (!file) return;

    showIdentityModal('Import Identity',
        '<div class="modal-field">' +
            '<label>File</label>' +
            '<span class="modal-value text-xs">' + escapeHtml(file.name) + ' (' + file.size + ' bytes)</span>' +
        '</div>' +
        '<div class="modal-field">' +
            '<label>Display Name</label>' +
            '<input type="text" id="identity-modal-nickname" class="modal-input" placeholder="e.g. Imported Key" maxlength="32">' +
        '</div>',
        function() {
            var nickname = document.getElementById('identity-modal-nickname').value.trim();
            // JSON-only IPC: base64 client-side instead of FormData upload.
            return file.arrayBuffer().then(function(buf) {
                var bytes = new Uint8Array(buf);
                var binary = '';
                for (var i = 0; i < bytes.length; i++) binary += String.fromCharCode(bytes[i]);
                var b64 = btoa(binary);
                return RS.invoke('api_import_identity_base64', {
                    args: { key: b64, nickname: nickname },
                });
            }).then(function(data) {
                var msg = data && data.reimported ? 'Identity re-imported' : 'Identity imported';
                showToast(msg, 'toast-green', 3000);
                closeIdentityModal();
            }).catch(function(err) {
                showToast(err && err.message ? err.message : 'Failed to import identity', 'toast-red', 3000);
            }).then(function() {
                loadIdentities();
            });
        }
    );
}

function exportActiveIdentity() {
    if (!activeIdentityHash) {
        showPreConditionToast('No active identity to export');
        return;
    }
    RS.invoke('api_export_identity_base64', { hashHex: activeIdentityHash }).then(function(data) {
        var raw = atob(data.key);
        var arr = new Uint8Array(raw.length);
        for (var i = 0; i < raw.length; i++) arr[i] = raw.charCodeAt(i);
        var blob = new Blob([arr], { type: 'application/octet-stream' });
        var url = URL.createObjectURL(blob);
        var a = document.createElement('a');
        a.href = url;
        a.download = activeIdentityHash.substring(0, 16) + '.rniid';
        a.style.display = 'none';
        document.body.appendChild(a);
        a.click();
        a.remove();
        setTimeout(function() { try { URL.revokeObjectURL(url); } catch (_) {} }, 60000);
    }).catch(function(err) {
        showToast(err && err.message ? err.message : 'Export failed', 'toast-red', 3000);
    });
}

function removeSelectedIdentity() {
    if (!selectedIdentityHash || selectedIdentityHash === activeIdentityHash) {
        showPreConditionToast('Cannot remove the active identity');
        return;
    }
    var sorted = identityList.slice().sort(function(a, b) {
        return (a.created_at || 0) - (b.created_at || 0);
    });
    if (sorted.length > 0 && selectedIdentityHash === sorted[0].hash) {
        showPreConditionToast('Cannot remove the original identity');
        return;
    }

    rsConfirm({
        title: 'Delete Identity',
        message: 'This identity will be removed from your device completely, and can not be recovered. Are you sure you want to delete it?',
        confirmText: 'Delete',
        danger: true
    }).then(function(ok) {
        if (!ok) return;
        return rsChoice({
            title: 'Delete Identity Data?',
            message: 'Do you also want to remove any stored contacts, messages, or other data related to this identity?',
            choices: [
                { label: 'Yes, delete everything', value: 'cascade', danger: true, hint: 'Contacts, messages, games, and all other data will be permanently deleted.' },
                { label: 'No, just the identity', value: 'keep', hint: 'Data will be preserved and reappear if this identity is re-imported.' }
            ]
        });
    }).then(function(choice) {
        if (!choice) return;
        var cascade = (choice === 'cascade');
        RS.invoke('api_delete_identity', { hashHex: selectedIdentityHash, cascade: cascade })
            .then(function() {
                showToast('Identity deleted', 'toast-green', 3000);
                selectedIdentityHash = null;
                loadIdentities();
            }).catch(function(err) {
                showToast(err && err.message ? err.message : 'Failed to delete identity', 'toast-red', 3000);
            });
    });
}

function deleteActiveIdentity() {
    if (identityList.length <= 1) {
        showPreConditionToast('Cannot delete the only identity');
        return;
    }

    var firstRemaining = null;
    for (var j = 0; j < identityList.length; j++) {
        if (identityList[j].hash !== activeIdentityHash) {
            firstRemaining = identityList[j];
            break;
        }
    }
    if (!firstRemaining) {
        showPreConditionToast('No other identity to switch to');
        return;
    }

    rsConfirm({
        title: 'Delete Identity',
        message: 'This identity will be removed from your device completely, and can not be recovered. Are you sure you want to delete it?',
        confirmText: 'Delete',
        danger: true
    }).then(function(ok) {
        if (!ok) return;
        return rsChoice({
            title: 'Delete Identity Data?',
            message: 'Do you also want to remove any stored contacts, messages, or other data related to this identity?',
            choices: [
                { label: 'Yes, delete everything', value: 'cascade', danger: true, hint: 'Contacts, messages, games, and all other data will be permanently deleted.' },
                { label: 'No, just the identity', value: 'keep', hint: 'Data will be preserved and reappear if this identity is re-imported.' }
            ]
        });
    }).then(function(choice) {
        if (!choice) return;
        var cascade = (choice === 'cascade');
        var hashToDelete = activeIdentityHash;
        // Switch to another identity first, then delete the old one.
        RS.invoke('api_activate_identity', { hashHex: firstRemaining.hash })
            .then(function() {
                activeIdentityHash = firstRemaining.hash;
                return RS.invoke('api_delete_identity', { hashHex: hashToDelete, cascade: cascade });
            })
            .then(function() {
                showToast('Identity deleted', 'toast-green', 3000);
                selectedIdentityHash = null;
                loadIdentities();
            })
            .catch(function(err) {
                showToast(err && err.message ? err.message : 'Failed to delete identity', 'toast-red', 3000);
            });
    });
}

function showIdentityModal(title, bodyHtml, onConfirm, confirmClass) {
    var modal = document.getElementById('identity-modal');
    if (!modal) return;

    document.getElementById('identity-modal-title').textContent = title;
    document.getElementById('identity-modal-body').innerHTML = bodyHtml;

    var confirmBtn = document.getElementById('identity-modal-confirm');
    confirmBtn.className = confirmClass || 'nr-btn';
    confirmBtn.disabled = false;
    var baseLabel = title.indexOf('Delete') !== -1 ? 'Delete' : (title.indexOf('Remove') !== -1 ? 'Remove' : (title.indexOf('Import') !== -1 ? 'Import' : 'Create'));
    confirmBtn.textContent = baseLabel;
    confirmBtn.dataset.baseLabel = baseLabel;
    confirmBtn.onclick = function() {
        if (confirmBtn.disabled) return;
        confirmBtn.disabled = true;
        confirmBtn.textContent = 'Working\u2026';
        var restore = function() {
            confirmBtn.disabled = false;
            confirmBtn.textContent = confirmBtn.dataset.baseLabel || baseLabel;
        };
        var result;
        try {
            result = onConfirm && onConfirm();
        } catch (e) {
            restore();
            throw e;
        }
        if (result && typeof result.then === 'function') {
            var done = false;
            var settle = function() { if (!done) { done = true; restore(); } };
            result.then(settle, settle);
        }
    };

    var overlay = document.getElementById('identity-modal-overlay');
    modal.classList.add('open');
    if (overlay) overlay.classList.add('active');

    modal.onkeydown = function(e) {
        if (e.key === 'Enter' && e.target.tagName === 'INPUT') {
            e.preventDefault();
            var btn = document.getElementById('identity-modal-confirm');
            if (btn) btn.click();
        }
    };

    setTimeout(function() {
        var input = modal.querySelector('.modal-input');
        if (input && !isMobile()) input.focus();
    }, 100);
}

function closeIdentityModal() {
    var modal = document.getElementById('identity-modal');
    var overlay = document.getElementById('identity-modal-overlay');
    if (modal) modal.classList.remove('open');
    if (overlay) overlay.classList.remove('active');
    var confirmBtn = document.getElementById('identity-modal-confirm');
    if (confirmBtn) {
        confirmBtn.disabled = false;
        if (confirmBtn.dataset.baseLabel) confirmBtn.textContent = confirmBtn.dataset.baseLabel;
    }
}

document.addEventListener('DOMContentLoaded', function() {
    var fileInput = document.getElementById('identity-file-input');
    if (fileInput) {
        fileInput.addEventListener('change', function() {
            if (this.files && this.files[0]) {
                handleImportFile(this.files[0]);
                this.value = '';
            }
        });
    }

    var identityAddBtn = document.getElementById('identity-add-btn');
    if (identityAddBtn) identityAddBtn.addEventListener('click', createNewIdentity);

    var identityDeleteBtn = document.getElementById('identity-delete-btn');
    if (identityDeleteBtn) identityDeleteBtn.addEventListener('click', deleteActiveIdentity);

    var modalClose = document.getElementById('identity-modal-close');
    if (modalClose) modalClose.addEventListener('click', closeIdentityModal);

    var modalCancel = document.getElementById('identity-modal-cancel');
    if (modalCancel) modalCancel.addEventListener('click', closeIdentityModal);

    if (typeof initSheetSwipeDismiss === 'function') {
        initSheetSwipeDismiss('identity-modal', 'identity-modal-overlay', closeIdentityModal);
    }

    document.addEventListener('keydown', function(e) {
        if (e.key === 'Escape') {
            var modal = document.getElementById('identity-modal');
            if (modal && modal.classList.contains('open')) {
                closeIdentityModal();
            }
        }
    });
});

RS.listen('identity_switched', function(data) {
    // Suppress the redundant loadConversations() that lxmf_identity triggers —
    // emit_initial_state fires lxmf_identity right after identity_switched.
    window._identitySwitchInProgress = true;

    activeIdentityHash = data.hash;

    if (data.lxmf_hash && typeof updateHeaderIdentity === 'function') {
        updateHeaderIdentity(data.lxmf_hash, data.display_name || '');
    }

    loadIdentities();

    // Clear identity-scoped frontend state so the old identity's data
    // doesn't leak. PeersCache rehydrates from the new snapshot on activation.
    if (typeof lxmfContacts !== 'undefined') lxmfContacts = [];
    if (typeof contactIdentityStatus !== 'undefined') contactIdentityStatus = {};

    if (typeof lxmfConversation !== 'undefined') lxmfConversation = [];
    if (typeof _conversationCache !== 'undefined') {
        for (var k in _conversationCache) delete _conversationCache[k];
    }
    if (typeof _cacheLru !== 'undefined') _cacheLru = [];

    if (typeof lxmfActiveContact !== 'undefined') lxmfActiveContact = null;
    if (typeof lxmfPendingFile !== 'undefined') lxmfPendingFile = null;
    if (typeof lxmfIdentity !== 'undefined') lxmfIdentity = null;
    if (typeof _ghostConversationHash !== 'undefined') _ghostConversationHash = null;
    if (typeof _replyTarget !== 'undefined') _replyTarget = null;
    if (typeof _msgReactions !== 'undefined') _msgReactions = {};
    if (typeof _lxmfDrafts !== 'undefined') _lxmfDrafts = {};

    if (typeof lxmfIdentityHash !== 'undefined') lxmfIdentityHash = data.hash;

    if (typeof events !== 'undefined') events = [];
    if (typeof activityLog !== 'undefined') activityLog = [];

    var msgList = document.getElementById('lxmf-messages');
    if (msgList) msgList.innerHTML = '<div class="lxmf-empty">Select a contact to view conversation.</div>';
    var chatHeader = document.getElementById('lxmf-chat-header');
    if (chatHeader) chatHeader.style.display = 'none';

    if (typeof _conversationsFirstLoadDone !== 'undefined') _conversationsFirstLoadDone = false;
    if (typeof _lastConversationsLoad !== 'undefined') _lastConversationsLoad = 0;
    if (typeof loadConversations === 'function') loadConversations();

    if (typeof gamesTabClear === 'function') gamesTabClear();

    if (typeof renderActivityFeed === 'function') renderActivityFeed();
    if (typeof renderLog === 'function') renderLog();

    setTimeout(function() { window._identitySwitchInProgress = false; }, 2000);

    showToast('Identity switched', 'toast-green', 3000);
});
