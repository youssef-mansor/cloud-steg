// ============== State ==============
let currentUser = null;
let currentTab = 'myImages';

// ============== DOM Elements ==============
const loginScreen = document.getElementById('loginScreen');
const dashboardScreen = document.getElementById('dashboardScreen');
const usernameInput = document.getElementById('usernameInput');
const loginBtn = document.getElementById('loginBtn');
const logoutBtn = document.getElementById('logoutBtn');
const usernameDisplay = document.getElementById('usernameDisplay');
const heartbeatStatus = document.getElementById('heartbeatStatus');

// Tabs
const tabBtns = document.querySelectorAll('.tab-btn');
const tabPanes = document.querySelectorAll('.tab-pane');

// My Images Tab
const imageUpload = document.getElementById('imageUpload');
const myImagesGrid = document.getElementById('myImagesGrid');

// Browse Tab
const refreshUsersBtn = document.getElementById('refreshUsersBtn');
const usersList = document.getElementById('usersList');

// Requests Tab
const refreshRequestsBtn = document.getElementById('refreshRequestsBtn');
const requestsList = document.getElementById('requestsList');

// Viewable Tab
const refreshViewableBtn = document.getElementById('refreshViewableBtn');
const viewableList = document.getElementById('viewableList');

// Modals
const imageModal = document.getElementById('imageModal');
const modalImage = document.getElementById('modalImage');
const modalTitle = document.getElementById('modalTitle');
const modalDetails = document.getElementById('modalDetails');

const requestModal = document.getElementById('requestModal');
const requestImageName = document.getElementById('requestImageName');
const requestUsername = document.getElementById('requestUsername');
const sendRequestBtn = document.getElementById('sendRequestBtn');

const approveModal = document.getElementById('approveModal');
const approveImageName = document.getElementById('approveImageName');
const approveUsername = document.getElementById('approveUsername');
const viewCountInput = document.getElementById('viewCountInput');
const coverImageInput = document.getElementById('coverImageInput');
const approveBtn = document.getElementById('approveBtn');

// ============== API Helpers ==============

async function apiCall(endpoint, options = {}) {
    try {
        const response = await fetch(endpoint, {
            ...options,
            headers: {
                'Content-Type': 'application/json',
                ...options.headers
            }
        });

        if (!response.ok) {
            const error = await response.json().catch(() => ({ error: 'Request failed' }));
            throw new Error(error.error || 'Request failed');
        }

        return await response.json();
    } catch (e) {
        console.error('API Error:', e);
        alert(e.message);
        throw e;
    }
}

// ============== Authentication ==============

async function register() {
    const username = usernameInput.value.trim();

    if (!username) {
        alert('Please enter a username');
        return;
    }

    try {
        const data = await apiCall('/api/register', {
            method: 'POST',
            body: JSON.stringify({ username })
        });

        alert(`✅ User '${data.username || username}' registered successfully! You can now login.`);

    } catch (e) {
        console.error('Registration failed:', e);
    }
}

async function login() {
    const username = usernameInput.value.trim();

    if (!username) {
        alert('Please enter a username');
        return;
    }

    try {
        const data = await apiCall('/api/login', {
            method: 'POST',
            body: JSON.stringify({ username })
        });

        currentUser = data.username;
        usernameDisplay.textContent = `@${currentUser}`;

        loginScreen.classList.add('hidden');
        dashboardScreen.classList.remove('hidden');

        // Load initial data
        loadMyImages();
        loadOnlineUsers();
        displayApprovedRequests(); // Load approved requests from localStorage

    } catch (e) {
        console.error('Login failed:', e);
    }
}

async function logout() {
    try {
        await apiCall('/api/logout', { method: 'POST' });
        currentUser = null;
        loginScreen.classList.remove('hidden');
        dashboardScreen.classList.add('hidden');
        usernameInput.value = '';
    } catch (e) {
        console.error('Logout failed:', e);
    }
}

// ============== Tab Navigation ==============

function switchTab(tabName) {
    currentTab = tabName;

    tabBtns.forEach(btn => {
        if (btn.dataset.tab === tabName) {
            btn.classList.add('active');
        } else {
            btn.classList.remove('active');
        }
    });

    tabPanes.forEach(pane => {
        if (pane.id === `${tabName}Tab`) {
            pane.classList.add('active');
        } else {
            pane.classList.remove('active');
        }
    });

    // Load data when switching tabs
    switch (tabName) {
        case 'myImages':
            loadMyImages();
            break;
        case 'browse':
            loadOnlineUsers();
            break;
        case 'requests':
            loadRequests();
            break;
        case 'viewable':
            loadViewableImages();
            break;
    }
}

// ============== My Images Tab ==============

async function loadMyImages() {
    try {
        const data = await apiCall('/api/my-images');

        // Only show local originals (not thumbnails from server)
        const images = data.local_images || [];

        if (images.length === 0) {
            myImagesGrid.innerHTML = '<p class="empty-state">No images yet. Upload your first image!</p>';
            return;
        }

        myImagesGrid.innerHTML = images.map(filename => `
      <div class="image-card" onclick="viewImage('${currentUser}', '${filename}')">
        <img src="/api/image/${currentUser}/${filename}" alt="${filename}" />
        <div class="image-card-info">
          <p>${filename}</p>
        </div>
      </div>
    `).join('');

    } catch (e) {
        myImagesGrid.innerHTML = '<p class="empty-state">Failed to load images</p>';
    }
}

async function uploadImage() {
    const file = imageUpload.files[0];
    if (!file) return;

    // Validate image dimensions
    const img = new Image();
    img.src = URL.createObjectURL(file);

    img.onload = async () => {
        URL.revokeObjectURL(img.src);

        // Image will be resized on server, so just upload
        const formData = new FormData();
        formData.append('image', file);

        try {
            const response = await fetch('/api/upload', {
                method: 'POST',
                body: formData
            });

            if (!response.ok) {
                const error = await response.json();
                throw new Error(error.error || 'Upload failed');
            }

            const data = await response.json();
            alert(`✅ Image uploaded: ${data.filename}`);

            // Reload images
            loadMyImages();
            imageUpload.value = '';

        } catch (e) {
            alert(`❌ Upload failed: ${e.message}`);
        }
    };
}

// ============== Browse Tab ==============

async function loadOnlineUsers() {
    try {
        const data = await apiCall('/api/discover');

        if (data.count === 0) {
            usersList.innerHTML = '<p class="empty-state">No users online</p>';
            return;
        }

        // Remember which users were expanded
        const expandedUsers = new Set();
        document.querySelectorAll('.user-item.expanded').forEach(item => {
            expandedUsers.add(item.id.replace('user-', ''));
        });

        usersList.innerHTML = data.online_clients.map(user => `
      <div class="user-item ${expandedUsers.has(user.username) ? 'expanded' : ''}" id="user-${user.username}">
        <div class="user-header" onclick="toggleUser('${user.username}')">
          <div class="user-info">
            <div class="user-avatar">${user.username[0].toUpperCase()}</div>
            <div>
              <div class="user-name">${user.username}</div>
              <div class="user-status">● Online</div>
            </div>
          </div>
          <span class="expand-icon">▼</span>
        </div>
        <div class="user-images" id="images-${user.username}">
          <div class="image-grid">
            <p class="empty-state">Loading...</p>
          </div>
        </div>
      </div>
    `).join('');

        // Reload images for expanded users
        expandedUsers.forEach(username => {
            if (data.online_clients.find(u => u.username === username)) {
                loadUserImages(username);
            }
        });

    } catch (e) {
        usersList.innerHTML = '<p class="empty-state">Failed to load users</p>';
    }
}

async function loadUserImages(username) {
    const imagesContainer = document.getElementById(`images-${username}`);
    if (!imagesContainer) return;

    try {
        const data = await apiCall(`/api/user-images/${username}`);

        if (data.count === 0) {
            imagesContainer.innerHTML = '<p class="empty-state">No images</p>';
            return;
        }

        imagesContainer.innerHTML = `
      <div class="image-grid">
        ${data.images.map(filename => `
          <div class="image-card" onclick="requestImageAccess('${username}', '${filename}')">
            <img src="/api/image/${username}/${filename}" alt="${filename}" />
            <div class="image-card-info">
              <p>${filename}</p>
            </div>
          </div>
        `).join('')}
      </div>
    `;
    } catch (e) {
        imagesContainer.innerHTML = '<p class="empty-state">Failed to load images</p>';
    }
}

async function toggleUser(username) {
    const userItem = document.getElementById(`user-${username}`);
    const imagesContainer = document.getElementById(`images-${username}`);

    if (userItem.classList.contains('expanded')) {
        userItem.classList.remove('expanded');
        return;
    }

    userItem.classList.add('expanded');
    loadUserImages(username);
}

function expandAllUsers() {
    const userItems = document.querySelectorAll('.user-item');
    userItems.forEach(async (item) => {
        if (!item.classList.contains('expanded')) {
            const username = item.id.replace('user-', '');
            await toggleUser(username);
        }
    });
}

function collapseAllUsers() {
    const userItems = document.querySelectorAll('.user-item');
    userItems.forEach(item => {
        item.classList.remove('expanded');
    });
}

// ============== Request Access ==============

function requestImageAccess(username, filename) {
    // Convert thumbnail filename to original filename for approval
    // thumbnail: 123-thumb-file.png -> original: 123-original-file.png
    const originalFilename = filename.replace('-thumb-', '-original-').replace(/\.png$/, function (match) {
        // If it was already .png, keep it, otherwise the original might have different extension
        return match;
    });

    requestUsername.textContent = username;
    requestImageName.textContent = originalFilename;
    requestModal.classList.remove('hidden');
}

async function sendViewRequest() {
    const username = requestUsername.textContent;
    const image = requestImageName.textContent;

    try {
        await apiCall('/api/request-view', {
            method: 'POST',
            body: JSON.stringify({ username, image })
        });

        alert('✅ Request sent!');
        requestModal.classList.add('hidden');

    } catch (e) {
        alert('❌ Failed to send request');
    }
}

// ============== Requests Tab ==============

async function loadRequests() {
    try {
        const data = await apiCall('/api/requests');

        if (data.requests.length === 0) {
            requestsList.innerHTML = '<p class="empty-state">No pending requests</p>';
            document.getElementById('requestCount').textContent = '0';
            window.currentRequests = [];
            return;
        }

        window.currentRequests = data.requests;
        document.getElementById('requestCount').textContent = data.requests.length;

        requestsList.innerHTML = data.requests.map((req, idx) => `
      <div class="request-card">
        <div class="request-info">
          <h3>Request from: ${req.from}</h3>
          <p>Image: ${req.image}</p>
        </div>
        <div class="request-actions">
          <button class="btn-primary" onclick="approveRequest(${idx})">Approve</button>
          <button class="btn-secondary" onclick="rejectRequest(${idx})">Reject</button>
        </div>
      </div>
    `).join('');

    } catch (e) {
        requestsList.innerHTML = '<p class="empty-state">Failed to load requests</p>';
    }
}

function approveRequest(idx) {
    const req = window.currentRequests[idx];
    if (!req) {
        alert('Request not found');
        return;
    }

    approveImageName.textContent = req.image;
    approveUsername.textContent = req.from;
    viewCountInput.value = 5;
    coverImageInput.value = '';

    approveModal.dataset.requestId = `${Date.now()}-${req.from}.json`;
    approveModal.dataset.image = req.image;  // Store for recordApproval
    approveModal.dataset.requester = req.from;  // Store for recordApproval
    approveModal.classList.remove('hidden');
}

async function rejectRequest(idx) {
    if (!confirm('Reject this request?')) {
        return;
    }

    const requests = window.currentRequests || [];
    const request = requests[idx];

    if (!request) {
        alert('Request not found');
        return;
    }

    try {
        const response = await fetch('/api/reject', {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ requestId: request.id })
        });

        if (!response.ok) {
            throw new Error('Rejection failed');
        }

        alert('✅ Request rejected');
        loadRequests();

    } catch (e) {
        alert(`❌ Failed to reject: ${e.message}`);
    }
}

async function approveWithCover() {
    const coverFile = coverImageInput.files[0];
    if (!coverFile) {
        alert('Please select a cover image');
        return;
    }

    const viewCount = viewCountInput.value;
    if (!viewCount || viewCount < 1) {
        alert('Please enter a valid view count');
        return;
    }

    // Get the request ID and details from the modal
    const requestId = approveModal.dataset.requestId;

    if (!requestId) {
        alert('No request selected');
        return;
    }

    try {
        const formData = new FormData();
        formData.append('requestId', requestId);
        formData.append('viewCount', viewCount);
        formData.append('coverImage', coverFile);

        const response = await fetch('/api/approve', {
            method: 'POST',
            body: formData
        });

        if (!response.ok) {
            const error = await response.json();
            throw new Error(error.error || 'Approval failed');
        }

        const data = await response.json();
        alert(`✅ ${data.message}`);

        // Record approval for later editing (use modal data, not currentRequests which is cleared)
        const approvedImage = approveModal.dataset.image;
        const approvedRecipient = approveModal.dataset.requester;
        recordApproval(approvedRecipient, approvedImage, parseInt(viewCountInput.value));

        approveModal.classList.add('hidden');
        loadRequests();
        displayApprovedRequests(); // Refresh approved list

    } catch (e) {
        alert(`❌ Approval failed: ${e.message}`);
    }
}

// ============== Viewable Images Tab ==============

async function loadViewableImages() {
    try {
        const data = await apiCall('/api/viewable');

        if (data.images.length === 0) {
            viewableList.innerHTML = '<p class="empty-state">No shared images yet</p>';
            document.getElementById('viewableCount').textContent = '0';
            window.viewableImages = [];
            return;
        }

        window.viewableImages = data.images;
        document.getElementById('viewableCount').textContent = data.images.length;

        viewableList.innerHTML = data.images.map((img, idx) => `
      <div class="viewable-card">
        <div class="viewable-info">
          <h3>${img.originalImage}</h3>
          <p>From: ${img.from}</p>
          <p>Views remaining: <span class="view-count">${img.viewCount}</span></p>
        </div>
        <button class="btn-primary" onclick="viewStegImage(${idx})">VIEW</button>
      </div>
    `).join('');

    } catch (e) {
        viewableList.innerHTML = '<p class="empty-state">Failed to load images</p>';
    }
}

async function viewStegImage(idx) {
    const images = window.viewableImages || [];
    const image = images[idx];

    if (!image) {
        alert('Image not found');
        return;
    }

    try {
        const response = await fetch('/api/view-image', {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ filename: image.filename })
        });

        if (!response.ok) {
            const error = await response.json();
            throw new Error(error.error || 'Failed to decrypt image');
        }

        const imageBlob = await response.blob();
        const imageUrl = URL.createObjectURL(imageBlob);

        // Display in modal
        modalImage.src = imageUrl;
        modalTitle.textContent = `From: ${image.from}`;
        modalDetails.textContent = `Original: ${image.originalImage} | Views left: ${image.viewCount - 1}`;
        imageModal.classList.remove('hidden');

        // Reload viewable list to update count
        setTimeout(() => loadViewableImages(), 1000);

    } catch (e) {
        alert(`❌ Failed to view image: ${e.message}`);
    }
}

// ============== Image Viewer ==============

function viewImage(username, filename) {
    modalImage.src = `/api/image/${username}/${filename}`;
    modalTitle.textContent = filename;
    modalDetails.textContent = `Owner: ${username}`;
    imageModal.classList.remove('hidden');
}

// ============== Event Listeners ==============

const registerBtn = document.getElementById('registerBtn');

loginBtn.addEventListener('click', login);
if (registerBtn) registerBtn.addEventListener('click', register);
logoutBtn.addEventListener('click', logout);

usernameInput.addEventListener('keypress', (e) => {
    if (e.key === 'Enter') login();
});

tabBtns.forEach(btn => {
    btn.addEventListener('click', () => switchTab(btn.dataset.tab));
});

imageUpload.addEventListener('change', uploadImage);

const expandAllBtn = document.getElementById('expandAllBtn');
const collapseAllBtn = document.getElementById('collapseAllBtn');

refreshUsersBtn.addEventListener('click', loadOnlineUsers);
expandAllBtn.addEventListener('click', expandAllUsers);
collapseAllBtn.addEventListener('click', collapseAllUsers);
refreshRequestsBtn.addEventListener('click', loadRequests);
refreshViewableBtn.addEventListener('click', loadViewableImages);
document.getElementById('refreshApprovedBtn').addEventListener('click', displayApprovedRequests);
document.getElementById('updateViewCountBtn').addEventListener('click', updateViewCount);

sendRequestBtn.addEventListener('click', sendViewRequest);
approveBtn.addEventListener('click', approveWithCover);

// Modal close buttons
document.querySelectorAll('.modal-close').forEach(btn => {
    btn.addEventListener('click', () => {
        btn.closest('.modal').classList.add('hidden');
    });
});

// Add modal close handlers for editViewCountModal
document.getElementById('editViewCountModal').addEventListener('click', (e) => {
    if (e.target === document.getElementById('editViewCountModal') || e.target.classList.contains('modal-close')) {
        document.getElementById('editViewCountModal').classList.add('hidden');
    }
});

// Close modals on outside click
[imageModal, requestModal, approveModal].forEach(modal => {
    modal.addEventListener('click', (e) => {
        if (e.target === modal) {
            modal.classList.add('hidden');
        }
    });
});

// ============== Initialize ==============

// Auto-refresh tabs every second when logged in (except browse tab)
setInterval(() => {
    if (!currentUser) return;

    switch (currentTab) {
        case 'myImages':
            loadMyImages();
            break;
        case 'browse':
            // Don't auto-refresh browse tab - use manual refresh button only
            break;
        case 'requests':
            loadRequests();
            break;
        case 'viewable':
            loadViewableImages();
            break;
    }
}, 1000);

console.log('🚀 Cloud Steg User Dashboard loaded');
// Track approved requests (stored when approving)
let approvedRequests = [];

// Load approved requests from localStorage
function loadApprovedRequests() {
    const stored = localStorage.getItem('approvedRequests');
    return stored ? JSON.parse(stored) : [];
}

// Save approved requests to localStorage
function saveApprovedRequests(requests) {
    localStorage.setItem('approvedRequests', JSON.stringify(requests));
}

// Display approved requests
async function displayApprovedRequests() {
    const approvedList = document.getElementById('approvedList');
    approvedRequests = loadApprovedRequests();

    if (!approvedRequests || approvedRequests.length === 0) {
        approvedList.innerHTML = '<p class="empty-state">No approved requests</p>';
        return;
    }

    let html = '';
    for (const req of approvedRequests) {
        html += `
            <div class="request-item">
                <div class="request-info">
                    <p><strong>Image:</strong> ${req.image}</p>
                    <p><strong>Recipient:</strong> ${req.recipient}</p>
                    <p><strong>View Count:</strong> ${req.viewCount}</p>
                    <p><strong>Approved:</strong> ${new Date(req.timestamp).toLocaleString()}</p>
                </div>
                <div class="request-actions">
                    <button class="btn-secondary" onclick="editViewCount('${req.recipient}', '${req.image}', ${req.viewCount})">
                        ✏️ Edit View Count
                    </button>
                </div>
            </div>
        `;
    }
    approvedList.innerHTML = html;
}

// Edit view count for an approved request
function editViewCount(recipient, image, currentCount) {
    document.getElementById('editImageName').textContent = image;
    document.getElementById('editRecipient').textContent = recipient;
    document.getElementById('newViewCountInput').value = currentCount;

    const modal = document.getElementById('editViewCountModal');
    modal.classList.remove('hidden');

    // Store for update handler
    modal.dataset.recipient = recipient;
    modal.dataset.image = image;
}

// Update view count for a recipient
async function updateViewCount() {
    const modal = document.getElementById('editViewCountModal');
    const recipient = modal.dataset.recipient;
    const image = modal.dataset.image;
    const newCount = parseInt(document.getElementById('newViewCountInput').value);

    if (!newCount || newCount < 1) {
        alert('Please enter a valid view count');
        return;
    }

    try {
        const response = await fetch('/api/update-view-count', {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ recipient, image, viewCount: newCount })
        });

        const data = await response.json();

        if (!response.ok) throw new Error(data.error || 'Failed to update view count');

        // Update local storage
        approvedRequests = approvedRequests.map(req => {
            if (req.recipient === recipient && req.image === image) {
                return { ...req, viewCount: newCount };
            }
            return req;
        });
        saveApprovedRequests(approvedRequests);

        alert(`✅ View count updated successfully!`);
        modal.classList.add('hidden');
        displayApprovedRequests();
    } catch (e) {
        alert(`❌ Failed to update: ${e.message}`);
    }
}

// Call this after approval
function recordApproval(recipient, image, viewCount) {
    approvedRequests = loadApprovedRequests();
    approvedRequests.push({
        recipient,
        image,
        viewCount,
        timestamp: Date.now()
    });
    saveApprovedRequests(approvedRequests);
}
