import QtQuick 2.0
import QtQuick.Controls 1.0
import Panopticon 1.0

Item {
	property variant session: null

	Component {
		id: hexdump

		LinearView {
			anchors.fill: parent
			session: parent.session
		}
	}

	TabView {
		id: notebook
		anchors.fill: parent
	}

	Component.onCompleted: { notebook.addTab("Hexdump",hexdump) }
}
