import Foundation
import Network
import AppKit

final class LocalExtensionServer: @unchecked Sendable {
    private let listener: NWListener
    private let downloadController: DownloadController
    private let queue = DispatchQueue(label: "local.firelink.server")

    init?(downloadController: DownloadController) {
        self.downloadController = downloadController
        
        let port = NWEndpoint.Port(rawValue: 6412)!
        let parameters = NWParameters.tcp
        
        do {
            listener = try NWListener(using: parameters, on: port)
        } catch {
            print("Failed to create listener: \(error)")
            return nil
        }
    }

    func start() {
        listener.newConnectionHandler = { [weak self] connection in
            self?.handleConnection(connection)
        }
        listener.stateUpdateHandler = { state in
            print("LocalExtensionServer state: \(state)")
        }
        listener.start(queue: queue)
    }

    private func handleConnection(_ connection: NWConnection) {
        connection.start(queue: queue)
        
        connection.receive(minimumIncompleteLength: 1, maximumLength: 65536) { [weak self] data, _, isComplete, error in
            if let data = data, let requestString = String(data: data, encoding: .utf8) {
                self?.processRequest(requestString)
            }
            
            let response = """
            HTTP/1.1 200 OK\r
            Access-Control-Allow-Origin: *\r
            Access-Control-Allow-Methods: POST, OPTIONS\r
            Access-Control-Allow-Headers: Content-Type\r
            Content-Length: 0\r
            Connection: close\r
            \r\n
            """
            
            connection.send(content: response.data(using: .utf8), completion: .contentProcessed { _ in
                connection.cancel()
            })
        }
    }

    private func processRequest(_ request: String) {
        guard let range = request.range(of: "\r\n\r\n") else { return }
        
        let bodyString = request[range.upperBound...]
        guard let data = bodyString.data(using: .utf8) else { return }
        
        struct Payload: Decodable {
            let urls: [String]
            let referer: String?
        }
        
        do {
            let payload = try JSONDecoder().decode(Payload.self, from: data)
            Task { @MainActor in
                let text = payload.urls.joined(separator: "\n")
                if !text.isEmpty {
                    self.downloadController.pendingPasteboardText = text
                    NotificationCenter.default.post(name: NSNotification.Name("OpenAddDownloadsWindow"), object: nil)
                    NSApp.activate(ignoringOtherApps: true)
                }
            }
        } catch {
            print("Failed to parse local request JSON: \(error)")
        }
    }
}
